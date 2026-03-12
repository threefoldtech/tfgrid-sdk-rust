use std::any::TypeId;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::Engine as _;
use blake2::{Blake2b, Digest, digest::consts::U32};
use futures_util::{SinkExt, StreamExt};
use hmac::Hmac;
use pbkdf2::pbkdf2;
use prost::Message;
use rand::RngCore;
use reqwest::StatusCode;
use secp256k1::{
    PublicKey as SecpPublicKey, SecretKey as SecpSecretKey, ecdh::shared_secret_point,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Sha256, Sha512};
use subxt::{
    OnlineClient, PolkadotConfig,
    dynamic::{self, Value},
    tx::TxProgress,
    utils::AccountId32,
};
use subxt_signer::sr25519::Keypair;
use tokio::{
    net::TcpStream,
    sync::Mutex,
    time::{Instant, sleep},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message as WsMessage,
};
use url::Url;

use crate::{error::GridError, workloads, zos};

mod deployment;
mod types;
pub(crate) use deployment::{
    DeployDeployment, build_network, build_network_light, build_vm, build_vm_light,
    deployment_hash_hex, public_ip_count, sign_deployment, validate_vm_light_request,
    validate_vm_request,
};
pub use types::{
    DeploymentOutcome, ExistingNetworkSpec, FullNetworkSpec, FullNetworkSpecBuilder,
    FullNetworkTarget, NetworkLightSpec, NetworkLightSpecBuilder, NetworkTarget, NodePlacement,
    NodeRequirements, NodeRequirementsBuilder, VmDeployment, VmDeploymentBuilder,
    VmLightDeployment, VmLightDeploymentBuilder, VmLightMount, VmLightSpec, VmLightSpecBuilder,
    VmSpec, VmSpecBuilder, VolumeMountSpec,
};

const DEVNET_SUBSTRATE_URL: &str = "wss://tfchain.dev.grid.tf/ws";
const DEVNET_GRID_PROXY_URL: &str = "https://gridproxy.dev.grid.tf";
const DEVNET_RELAY_URL: &str = "wss://relay.dev.grid.tf";
const RMB_SCHEMA: &str = "application/json";

#[derive(Debug, Clone)]
pub struct GridClient {
    http: reqwest::Client,
    chain: OnlineClient<PolkadotConfig>,
    signer: Keypair,
    e2e_private_key: SecpSecretKey,
    identity: LiveIdentity,
    session_id: String,
    relay_socket: Arc<Mutex<Option<RelaySocket>>>,
    relay_url: String,
    grid_proxy_url: String,
    rmb_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct GridClientConfig {
    pub substrate_url: String,
    pub grid_proxy_url: String,
    pub relay_url: String,
    pub http_timeout: Duration,
    pub rmb_timeout: Duration,
}

impl Default for GridClientConfig {
    fn default() -> Self {
        Self::devnet()
    }
}

impl GridClientConfig {
    pub fn devnet() -> Self {
        Self {
            substrate_url: DEVNET_SUBSTRATE_URL.to_string(),
            grid_proxy_url: DEVNET_GRID_PROXY_URL.to_string(),
            relay_url: DEVNET_RELAY_URL.to_string(),
            http_timeout: Duration::from_secs(30),
            rmb_timeout: Duration::from_secs(30),
        }
    }

    pub fn builder() -> GridClientConfigBuilder {
        GridClientConfigBuilder {
            config: Self::devnet(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GridClientConfigBuilder {
    config: GridClientConfig,
}

impl GridClientConfigBuilder {
    pub fn substrate_url(mut self, substrate_url: impl Into<String>) -> Self {
        self.config.substrate_url = substrate_url.into();
        self
    }

    pub fn grid_proxy_url(mut self, grid_proxy_url: impl Into<String>) -> Self {
        self.config.grid_proxy_url = grid_proxy_url.into();
        self
    }

    pub fn relay_url(mut self, relay_url: impl Into<String>) -> Self {
        self.config.relay_url = relay_url.into();
        self
    }

    pub fn http_timeout(mut self, http_timeout: Duration) -> Self {
        self.config.http_timeout = http_timeout;
        self
    }

    pub fn rmb_timeout(mut self, rmb_timeout: Duration) -> Self {
        self.config.rmb_timeout = rmb_timeout;
        self
    }

    pub fn build(self) -> GridClientConfig {
        self.config
    }
}

type RelaySocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug, Clone)]
struct LiveIdentity {
    twin_id: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct ProxyNode {
    #[serde(rename = "nodeId")]
    node_id: u32,
    #[serde(rename = "twinId")]
    twin_id: u32,
    status: String,
    healthy: bool,
    features: Vec<String>,
    #[serde(rename = "farm_free_ips", default)]
    farm_free_ips: u64,
    #[serde(rename = "publicConfig", default)]
    public_config: ProxyPublicConfig,
    #[serde(rename = "total_resources")]
    total_resources: ProxyCapacity,
    #[serde(rename = "used_resources")]
    used_resources: ProxyCapacity,
}

impl ProxyNode {
    fn fixed(node_id: u32, twin_id: u32) -> Self {
        Self {
            node_id,
            twin_id,
            status: "up".to_string(),
            healthy: true,
            features: vec!["network-light".to_string(), "zmachine-light".to_string()],
            farm_free_ips: 0,
            public_config: ProxyPublicConfig::default(),
            total_resources: ProxyCapacity {
                cru: u64::MAX,
                mru: u64::MAX,
                sru: u64::MAX,
            },
            used_resources: ProxyCapacity {
                cru: 0,
                mru: 0,
                sru: 0,
            },
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ProxyPublicConfig {
    #[serde(default)]
    domain: String,
    #[serde(default)]
    gw4: String,
    #[serde(default)]
    gw6: String,
    #[serde(default)]
    ipv4: String,
    #[serde(default)]
    ipv6: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ProxyCapacity {
    cru: u64,
    mru: u64,
    sru: u64,
}

#[derive(Debug, Clone)]
struct DerivedNames {
    vm_name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ProxyTwin {
    #[serde(rename = "twinId")]
    twin_id: u32,
    relay: Option<String>,
    #[serde(rename = "publicKey")]
    public_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProxyContract {
    #[serde(rename = "contract_id")]
    contract_id: u64,
}

#[derive(Debug, Clone, Serialize)]
struct DeploymentMetadata<'a> {
    version: i32,
    #[serde(rename = "type")]
    kind: &'a str,
    name: &'a str,
    #[serde(rename = "projectName")]
    project_name: &'a str,
}

#[derive(Debug, Clone, Serialize)]
struct NetworkWorkloadMetadata {
    version: i32,
    user_accesses: Option<Vec<serde_json::Value>>,
}

#[derive(Clone, PartialEq, Message)]
struct RmbRequest {
    #[prost(string, tag = "1")]
    command: String,
}

#[derive(Clone, PartialEq, Message)]
struct RmbResponse {}

#[derive(Clone, PartialEq, Message)]
struct RmbError {
    #[prost(uint32, tag = "1")]
    code: u32,
    #[prost(string, tag = "2")]
    message: String,
}

#[derive(Clone, PartialEq, Message)]
struct RmbAddress {
    #[prost(uint32, tag = "1")]
    twin: u32,
    #[prost(string, optional, tag = "2")]
    connection: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
struct RmbEnvelope {
    #[prost(string, tag = "1")]
    uid: String,
    #[prost(string, optional, tag = "2")]
    tags: Option<String>,
    #[prost(uint64, tag = "3")]
    timestamp: u64,
    #[prost(uint64, tag = "4")]
    expiration: u64,
    #[prost(message, optional, tag = "5")]
    source: Option<RmbAddress>,
    #[prost(message, optional, tag = "6")]
    destination: Option<RmbAddress>,
    #[prost(oneof = "rmb_envelope::Message", tags = "7, 8, 12")]
    message: Option<rmb_envelope::Message>,
    #[prost(bytes = "vec", optional, tag = "9")]
    signature: Option<Vec<u8>>,
    #[prost(string, optional, tag = "10")]
    schema: Option<String>,
    #[prost(string, optional, tag = "11")]
    federation: Option<String>,
    #[prost(oneof = "rmb_envelope::Payload", tags = "13, 14")]
    payload: Option<rmb_envelope::Payload>,
    #[prost(string, repeated, tag = "17")]
    relays: Vec<String>,
}

mod rmb_envelope {
    use super::{RmbError, RmbRequest, RmbResponse};
    use prost::Oneof;

    #[derive(Clone, PartialEq, Oneof)]
    pub enum Message {
        #[prost(message, tag = "7")]
        Request(RmbRequest),
        #[prost(message, tag = "8")]
        Response(RmbResponse),
        #[prost(message, tag = "12")]
        Error(RmbError),
    }

    #[derive(Clone, PartialEq, Oneof)]
    pub enum Payload {
        #[prost(bytes, tag = "13")]
        Plain(Vec<u8>),
        #[prost(bytes, tag = "14")]
        Cipher(Vec<u8>),
    }
}

impl GridClient {
    pub async fn devnet(mnemonic: &str) -> Result<Self, GridError> {
        Self::new(mnemonic, GridClientConfig::devnet()).await
    }

    pub async fn new(mnemonic: &str, config: GridClientConfig) -> Result<Self, GridError> {
        let http = reqwest::Client::builder()
            .timeout(config.http_timeout)
            .build()
            .map_err(|err| GridError::backend(err.to_string()))?;
        let mnemonic = subxt_signer::bip39::Mnemonic::parse(mnemonic)
            .map_err(|err| GridError::backend(err.to_string()))?;
        let signer = Keypair::from_phrase(&mnemonic, None)
            .map_err(|err| GridError::backend(err.to_string()))?;
        let e2e_private_key = e2e_private_key_from_mnemonic(&mnemonic)?;
        let chain = OnlineClient::<PolkadotConfig>::from_url(&config.substrate_url)
            .await
            .map_err(|err| GridError::backend(err.to_string()))?;
        let account_id: AccountId32 =
            <Keypair as subxt::tx::Signer<PolkadotConfig>>::account_id(&signer);
        let account_id = account_id.to_string();
        let twin_id = fetch_own_twin_id(&http, &config.grid_proxy_url, &account_id).await?;
        let client = Self {
            http,
            chain,
            signer,
            e2e_private_key,
            identity: LiveIdentity { twin_id },
            session_id: random_uid(),
            relay_socket: Arc::new(Mutex::new(None)),
            relay_url: config.relay_url,
            grid_proxy_url: config.grid_proxy_url,
            rmb_timeout: config.rmb_timeout,
        };
        if std::env::var_os("TFGRID_DEBUG").is_some() {
            let public_key = SecpPublicKey::from_secret_key(
                &secp256k1::Secp256k1::new(),
                &client.e2e_private_key,
            );
            trace_step(format!(
                "derived e2e public key 0x{}",
                hex::encode(public_key.serialize())
            ));
        }
        client
            .ensure_twin_relay()
            .await
            .map_err(|err| GridError::backend(format!("ensure twin relay: {err}")))?;
        Ok(client)
    }

    pub async fn deploy_small_vm(
        &self,
        ssh_key: Option<&str>,
    ) -> Result<DeploymentOutcome, GridError> {
        let mut vm = VmLightSpec::default();
        if let Some(key) = ssh_key.filter(|value| !value.trim().is_empty()) {
            vm.env.insert("SSH_KEY".to_string(), key.trim().to_string());
        }
        self.deploy_vm_light(VmLightDeployment {
            placement: NodePlacement::default(),
            network: NetworkTarget::Create(NetworkLightSpec::default()),
            vm,
        })
        .await
    }

    pub async fn deploy_vm_light(
        &self,
        request: VmLightDeployment,
    ) -> Result<DeploymentOutcome, GridError> {
        validate_vm_light_request(&request)?;
        match &request.placement {
            NodePlacement::Auto(requirements) => {
                let node = self.pick_node(requirements).await?;
                self.deploy_vm_light_on_node(node, request).await
            }
            NodePlacement::Fixed {
                node_id,
                node_twin_id,
            } => {
                let node = ProxyNode::fixed(*node_id, *node_twin_id);
                self.deploy_vm_light_on_node(node, request).await
            }
        }
    }

    pub async fn deploy_vm(&self, request: VmDeployment) -> Result<DeploymentOutcome, GridError> {
        validate_vm_request(&request)?;
        match &request.placement {
            NodePlacement::Auto(requirements) => {
                let node = self
                    .pick_zmachine_node(
                        requirements,
                        request.vm.public_ipv4,
                        request.vm.public_ipv6,
                    )
                    .await?;
                self.deploy_vm_on_node(node, request).await
            }
            NodePlacement::Fixed {
                node_id,
                node_twin_id,
            } => {
                let node = ProxyNode::fixed(*node_id, *node_twin_id);
                self.deploy_vm_on_node(node, request).await
            }
        }
    }

    pub async fn deploy_vm_on_existing_network(
        &self,
        node_id: u32,
        node_twin_id: u32,
        network_name: &str,
        vm_ip: &str,
        ssh_key: Option<&str>,
    ) -> Result<DeploymentOutcome, GridError> {
        let mut vm = VmLightSpec::default();
        if let Some(key) = ssh_key.filter(|value| !value.trim().is_empty()) {
            vm.env.insert("SSH_KEY".to_string(), key.trim().to_string());
        }
        self.deploy_vm_light(VmLightDeployment {
            placement: NodePlacement::Fixed {
                node_id,
                node_twin_id,
            },
            network: NetworkTarget::Existing(ExistingNetworkSpec {
                name: network_name.to_string(),
                ip: vm_ip.to_string(),
            }),
            vm,
        })
        .await
    }

    pub fn debug_rmb_token(&self) -> Result<String, GridError> {
        jwt_token(&self.signer, self.identity.twin_id, None, 60)
    }

    pub fn twin_id(&self) -> u32 {
        self.identity.twin_id
    }

    pub async fn cancel_contract(&self, contract_id: u64) -> Result<(), GridError> {
        if contract_id == 0 {
            return Ok(());
        }
        submit_cancel_contract(&self.chain, &self.signer, contract_id)
            .await
            .map_err(|err| GridError::backend(format!("cancel contract {contract_id}: {err}")))
    }

    pub async fn cancel_deployment_outcome(
        &self,
        outcome: &DeploymentOutcome,
    ) -> Result<(), GridError> {
        if outcome.vm_contract_id != 0 {
            let _ = self
                .rmb_call::<_, serde_json::Value>(
                    outcome.node_twin_id,
                    "zos.deployment.delete",
                    &json!({ "contract_id": outcome.vm_contract_id }),
                    None,
                )
                .await;
            self.cancel_contract(outcome.vm_contract_id).await?;
        }
        if outcome.network_contract_id != 0 {
            let _ = self
                .rmb_call::<_, serde_json::Value>(
                    outcome.node_twin_id,
                    "zos.deployment.delete",
                    &json!({ "contract_id": outcome.network_contract_id }),
                    None,
                )
                .await;
            self.cancel_contract(outcome.network_contract_id).await?;
        }
        Ok(())
    }

    async fn deploy_vm_light_on_node(
        &self,
        node: ProxyNode,
        request: VmLightDeployment,
    ) -> Result<DeploymentOutcome, GridError> {
        trace_step(format!(
            "selected node {} twin {}",
            node.node_id, node.twin_id
        ));
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| GridError::backend(err.to_string()))?
            .as_secs();
        let deployment_names = derived_names(&request, suffix);
        let (network_name, vm_ip, network_contract_id) = match &request.network {
            NetworkTarget::Create(network_spec) => {
                let network_name = network_spec
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("rust_net_light_{suffix}"));
                let subnet = network_spec
                    .subnet
                    .clone()
                    .unwrap_or_else(|| format!("10.{}.2.0/24", 10 + (suffix % 180) as u8));
                let vm_ip = vm_ip_from_subnet(&subnet)?;
                let network = build_network_light(
                    &network_name,
                    &subnet,
                    network_spec
                        .mycelium_key
                        .clone()
                        .unwrap_or_else(|| random_bytes(zos::MYCELIUM_KEY_LEN)),
                );
                let network_metadata =
                    deployment_metadata(&network_name, "network-light", "Network");
                let mut network_deployment =
                    DeployDeployment::new(self.identity.twin_id, network_metadata, vec![network]);
                sign_deployment(&mut network_deployment, self.identity.twin_id, &self.signer)?;
                let network_hash = deployment_hash_hex(&network_deployment)?;
                debug_dump("network", &network_deployment, &network_hash);

                submit_create_node_contract(
                    &self.chain,
                    &self.signer,
                    node.node_id,
                    &network_deployment.metadata,
                    &network_hash,
                    public_ip_count(&network_deployment.workloads),
                )
                .await
                .map_err(|err| GridError::backend(format!("create network contract: {err}")))?;
                let network_contract_id =
                    self.wait_for_contract(node.node_id, &network_hash).await?;
                trace_step(format!("network contract id {network_contract_id}"));
                network_deployment.contract_id = network_contract_id;
                self.deploy_and_confirm(node.twin_id, &network_deployment)
                    .await
                    .map_err(|err| GridError::backend(format!("deploy network over RMB: {err}")))?;
                trace_step(format!(
                    "network deployment visible on node {network_contract_id}"
                ));
                self.wait_for_workloads(node.twin_id, network_contract_id)
                    .await
                    .map_err(|err| GridError::backend(format!("wait network workloads: {err}")))?;
                trace_step(format!(
                    "network workloads settled for contract {network_contract_id}"
                ));
                (network_name, vm_ip, network_contract_id)
            }
            NetworkTarget::Existing(existing) => (existing.name.clone(), existing.ip.clone(), 0),
        };

        let vm_name = deployment_names.vm_name;
        let vm = build_vm_light(&vm_name, &network_name, &vm_ip, &request.vm);
        let vm_metadata = deployment_metadata(&vm_name, "vm-light", &vm_name);
        let mut vm_deployment = DeployDeployment::new(self.identity.twin_id, vm_metadata, vm);
        sign_deployment(&mut vm_deployment, self.identity.twin_id, &self.signer)?;
        let vm_hash = deployment_hash_hex(&vm_deployment)?;
        debug_dump("vm", &vm_deployment, &vm_hash);

        submit_create_node_contract(
            &self.chain,
            &self.signer,
            node.node_id,
            &vm_deployment.metadata,
            &vm_hash,
            public_ip_count(&vm_deployment.workloads),
        )
        .await
        .map_err(|err| GridError::backend(format!("create vm contract: {err}")))?;
        let vm_contract_id = self.wait_for_contract(node.node_id, &vm_hash).await?;
        trace_step(format!("vm contract id {vm_contract_id}"));
        vm_deployment.contract_id = vm_contract_id;
        self.deploy_and_confirm(node.twin_id, &vm_deployment)
            .await
            .map_err(|err| GridError::backend(format!("deploy vm over RMB: {err}")))?;
        trace_step(format!("vm deployment visible on node {vm_contract_id}"));
        let vm_changes = self
            .wait_for_workloads(node.twin_id, vm_contract_id)
            .await
            .map_err(|err| GridError::backend(format!("wait vm workloads: {err}")))?;
        trace_step(format!(
            "vm workloads settled for contract {vm_contract_id}"
        ));
        let vm_state = self
            .rmb_call::<_, serde_json::Value>(
                node.twin_id,
                "zos.deployment.get",
                &json!({ "contract_id": vm_contract_id }),
                None,
            )
            .await
            .map_err(|err| GridError::backend(format!("load vm deployment from node: {err}")))?;
        let vm_workload = extract_workloads(vm_state)?
            .into_iter()
            .find(|workload| workload.name == vm_name)
            .ok_or_else(|| GridError::NotFound(format!("vm workload {vm_name}")))?;
        let result: serde_json::Value = vm_workload.result.data;
        let mycelium_ip = result
            .get("mycelium_ip")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let console_url = result
            .get("console_url")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        if vm_changes
            .iter()
            .any(|workload| workload.result.state == zos::STATE_ERROR)
        {
            return Err(GridError::backend("vm deployment entered error state"));
        }

        Ok(DeploymentOutcome {
            node_id: node.node_id,
            node_twin_id: node.twin_id,
            network_name,
            network_contract_id,
            vm_name,
            vm_contract_id,
            vm_ip,
            mycelium_ip,
            public_ipv4: String::new(),
            public_ipv6: String::new(),
            console_url,
        })
    }

    async fn deploy_vm_on_node(
        &self,
        node: ProxyNode,
        request: VmDeployment,
    ) -> Result<DeploymentOutcome, GridError> {
        trace_step(format!(
            "selected node {} twin {}",
            node.node_id, node.twin_id
        ));
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| GridError::backend(err.to_string()))?
            .as_secs();
        let vm_name = request
            .vm
            .name
            .clone()
            .unwrap_or_else(|| format!("rust_vm_{suffix}"));
        let (network_name, vm_ip, network_contract_id) = match &request.network {
            FullNetworkTarget::Create(network_spec) => {
                let network_name = network_spec
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("rust_net_{suffix}"));
                let ip_range = network_spec
                    .ip_range
                    .clone()
                    .unwrap_or_else(|| format!("10.{}.0.0/16", 10 + (suffix % 180) as u8));
                let subnet = network_spec
                    .subnet
                    .clone()
                    .unwrap_or_else(|| format!("10.{}.2.0/24", 10 + (suffix % 180) as u8));
                let vm_ip = vm_ip_from_subnet(&subnet)?;
                let wg_listen_port = match network_spec.wireguard_listen_port {
                    Some(port) => port,
                    None => self.get_free_wg_port(node.twin_id).await?,
                };
                let network = build_network(
                    &network_name,
                    &ip_range,
                    &subnet,
                    network_spec
                        .wireguard_private_key
                        .clone()
                        .unwrap_or_else(generate_wireguard_private_key),
                    wg_listen_port,
                    network_spec.mycelium_key.clone(),
                );
                let network_metadata = deployment_metadata(&network_name, "network", "Network");
                let mut network_deployment =
                    DeployDeployment::new(self.identity.twin_id, network_metadata, vec![network]);
                sign_deployment(&mut network_deployment, self.identity.twin_id, &self.signer)?;
                let network_hash = deployment_hash_hex(&network_deployment)?;
                debug_dump("network", &network_deployment, &network_hash);
                submit_create_node_contract(
                    &self.chain,
                    &self.signer,
                    node.node_id,
                    &network_deployment.metadata,
                    &network_hash,
                    public_ip_count(&network_deployment.workloads),
                )
                .await
                .map_err(|err| GridError::backend(format!("create network contract: {err}")))?;
                let network_contract_id =
                    self.wait_for_contract(node.node_id, &network_hash).await?;
                trace_step(format!("network contract id {network_contract_id}"));
                network_deployment.contract_id = network_contract_id;
                self.deploy_and_confirm(node.twin_id, &network_deployment)
                    .await
                    .map_err(|err| GridError::backend(format!("deploy network over RMB: {err}")))?;
                self.wait_for_workloads(node.twin_id, network_contract_id)
                    .await
                    .map_err(|err| GridError::backend(format!("wait network workloads: {err}")))?;
                (network_name, vm_ip, network_contract_id)
            }
            FullNetworkTarget::Existing(existing) => {
                (existing.name.clone(), existing.ip.clone(), 0)
            }
        };

        let vm_workloads = build_vm(&vm_name, &network_name, &vm_ip, &request.vm);
        let vm_metadata = deployment_metadata(&vm_name, "vm", &vm_name);
        let mut vm_deployment =
            DeployDeployment::new(self.identity.twin_id, vm_metadata, vm_workloads);
        sign_deployment(&mut vm_deployment, self.identity.twin_id, &self.signer)?;
        let vm_hash = deployment_hash_hex(&vm_deployment)?;
        debug_dump("vm", &vm_deployment, &vm_hash);
        submit_create_node_contract(
            &self.chain,
            &self.signer,
            node.node_id,
            &vm_deployment.metadata,
            &vm_hash,
            public_ip_count(&vm_deployment.workloads),
        )
        .await
        .map_err(|err| GridError::backend(format!("create vm contract: {err}")))?;
        let vm_contract_id = self.wait_for_contract(node.node_id, &vm_hash).await?;
        vm_deployment.contract_id = vm_contract_id;
        self.deploy_and_confirm(node.twin_id, &vm_deployment)
            .await
            .map_err(|err| GridError::backend(format!("deploy vm over RMB: {err}")))?;
        let vm_changes = self
            .wait_for_workloads(node.twin_id, vm_contract_id)
            .await
            .map_err(|err| GridError::backend(format!("wait vm workloads: {err}")))?;
        let vm_state = self
            .rmb_call::<_, serde_json::Value>(
                node.twin_id,
                "zos.deployment.get",
                &json!({ "contract_id": vm_contract_id }),
                None,
            )
            .await
            .map_err(|err| GridError::backend(format!("load vm deployment from node: {err}")))?;
        let workloads = extract_workloads(vm_state)?;
        let vm_workload = workloads
            .iter()
            .find(|workload| workload.name == vm_name)
            .cloned()
            .ok_or_else(|| GridError::NotFound(format!("vm workload {vm_name}")))?;
        let result: serde_json::Value = vm_workload.result.data;
        let mycelium_ip = result
            .get("mycelium_ip")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let console_url = result
            .get("console_url")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let (public_ipv4, public_ipv6) = resolve_public_ips_from_workloads(&workloads, &vm_name);
        if vm_changes
            .iter()
            .any(|workload| workload.result.state == zos::STATE_ERROR)
        {
            return Err(GridError::backend("vm deployment entered error state"));
        }
        Ok(DeploymentOutcome {
            node_id: node.node_id,
            node_twin_id: node.twin_id,
            network_name,
            network_contract_id,
            vm_name,
            vm_contract_id,
            vm_ip,
            mycelium_ip,
            public_ipv4,
            public_ipv6,
            console_url,
        })
    }

    async fn pick_node(&self, requirements: &NodeRequirements) -> Result<ProxyNode, GridError> {
        let response = self
            .http
            .get(format!("{}/nodes", self.grid_proxy_url))
            .query(&[("status", "up")])
            .send()
            .await
            .map_err(|err| GridError::backend(err.to_string()))?;
        let nodes: Vec<ProxyNode> = parse_json_response(response, "grid proxy /nodes").await?;
        nodes
            .into_iter()
            .find(|node| {
                node.status == "up"
                    && node.healthy
                    && has_feature(node, "network-light")
                    && has_feature(node, "zmachine-light")
                    && free_mru(node) >= requirements.min_memory_bytes
                    && free_sru(node) >= requirements.min_rootfs_bytes
                    && node
                        .total_resources
                        .cru
                        .saturating_sub(node.used_resources.cru)
                        >= requirements.min_cru
            })
            .ok_or_else(|| {
                GridError::NotFound(
                    "no devnet node with network-light + zmachine-light".to_string(),
                )
            })
    }

    async fn pick_zmachine_node(
        &self,
        requirements: &NodeRequirements,
        needs_public_ipv4: bool,
        needs_public_ipv6: bool,
    ) -> Result<ProxyNode, GridError> {
        let response = self
            .http
            .get(format!("{}/nodes", self.grid_proxy_url))
            .query(&[("status", "up")])
            .send()
            .await
            .map_err(|err| GridError::backend(err.to_string()))?;
        let nodes: Vec<ProxyNode> = parse_json_response(response, "grid proxy /nodes").await?;
        nodes
            .into_iter()
            .find(|node| {
                let has_base = node.status == "up"
                    && node.healthy
                    && has_feature(node, "network")
                    && has_feature(node, "zmachine")
                    && has_feature(node, "volume")
                    && free_mru(node) >= requirements.min_memory_bytes
                    && free_sru(node) >= requirements.min_rootfs_bytes
                    && node
                        .total_resources
                        .cru
                        .saturating_sub(node.used_resources.cru)
                        >= requirements.min_cru;
                if !has_base {
                    return false;
                }
                if needs_public_ipv4
                    && (!has_feature(node, "ip")
                        || node.farm_free_ips == 0
                        || !node_has_usable_public_ipv4(node))
                {
                    return false;
                }
                if needs_public_ipv6
                    && ((!has_feature(node, "ipv4") && !has_feature(node, "ip"))
                        || node.farm_free_ips == 0
                        || !node_has_usable_public_ipv6(node))
                {
                    return false;
                }
                true
            })
            .ok_or_else(|| {
                GridError::NotFound(
                    "no devnet node with network + zmachine capabilities".to_string(),
                )
            })
    }

    async fn get_free_wg_port(&self, node_twin_id: u32) -> Result<u16, GridError> {
        let used_ports: Vec<u16> = self
            .rmb_call(
                node_twin_id,
                "zos.network.list_wg_ports",
                &json!(null),
                None,
            )
            .await
            .map_err(|err| GridError::backend(format!("load wg ports: {err}")))?;
        for port in 1024u16..32767u16 {
            if !used_ports.contains(&port) {
                return Ok(port);
            }
        }
        Err(GridError::backend("no free wireguard port found"))
    }

    async fn ensure_twin_relay(&self) -> Result<(), GridError> {
        let twin = fetch_twin(&self.http, &self.grid_proxy_url, self.identity.twin_id).await?;
        let expected_relay = relay_host(&self.relay_url)?;
        if twin.relay.as_deref() == Some(expected_relay.as_str()) {
            return Ok(());
        }
        submit_update_twin_relay(&self.chain, &self.signer, &expected_relay).await
    }

    async fn wait_for_contract(
        &self,
        node_id: u32,
        deployment_hash: &str,
    ) -> Result<u64, GridError> {
        trace_step(format!(
            "waiting for chain contract on node {node_id} hash {deployment_hash}"
        ));
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            let response = self
                .http
                .get(format!("{}/contracts", self.grid_proxy_url))
                .query(&[
                    ("twin_id", self.identity.twin_id.to_string()),
                    ("node_id", node_id.to_string()),
                    ("deployment_hash", deployment_hash.to_string()),
                    ("state", "Created".to_string()),
                ])
                .send()
                .await
                .map_err(|err| GridError::backend(err.to_string()))?;
            if response.status() == StatusCode::OK {
                let contracts: Vec<ProxyContract> =
                    parse_json_response(response, "grid proxy /contracts").await?;
                if let Some(contract) = contracts.into_iter().next() {
                    trace_step(format!("found chain contract {}", contract.contract_id));
                    return Ok(contract.contract_id);
                }
            }
            if Instant::now() >= deadline {
                return Err(GridError::backend(format!(
                    "timed out waiting for contract {deployment_hash}"
                )));
            }
            sleep(Duration::from_secs(1)).await;
        }
    }

    async fn wait_for_workloads(
        &self,
        node_twin_id: u32,
        contract_id: u64,
    ) -> Result<Vec<zos::Workload>, GridError> {
        trace_step(format!(
            "waiting for workloads on node twin {node_twin_id} contract {contract_id}"
        ));
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            let changes: Vec<zos::Workload> = match self
                .rmb_call::<_, serde_json::Value>(
                    node_twin_id,
                    "zos.deployment.changes",
                    &json!({ "contract_id": contract_id }),
                    None,
                )
                .await
            {
                Ok(changes) => extract_workloads(changes)?,
                Err(GridError::Backend(message))
                    if message.contains("deployment not found")
                        || message.contains("contract not found")
                        || message
                            .contains("relay closed before zos.deployment.changes response")
                        || message.contains("rmb timeout for zos.deployment.changes") =>
                {
                    trace_step(format!(
                        "workload poll retry for contract {contract_id}: {message}"
                    ));
                    if let Some(workloads) = self
                        .workloads_from_get(node_twin_id, contract_id, deadline)
                        .await?
                    {
                        return Ok(workloads);
                    }
                    if Instant::now() >= deadline {
                        return Err(GridError::backend(format!(
                            "timed out waiting for deployment changes for contract {contract_id}"
                        )));
                    }
                    sleep(Duration::from_secs(2)).await;
                    continue;
                }
                Err(err) => return Err(err),
            };
            if !changes.is_empty() {
                let states = changes
                    .iter()
                    .map(|workload| format!("{}={}", workload.name, workload.result.state))
                    .collect::<Vec<_>>()
                    .join(", ");
                trace_step(format!(
                    "workload states for contract {contract_id}: {states}"
                ));
            }
            let mut ok_workloads: HashMap<String, bool> = HashMap::new();
            for workload in &changes {
                match workload.result.state.as_str() {
                    "ok" => {
                        ok_workloads.insert(workload.name.clone(), true);
                    }
                    "error" | "deleted" | "paused" | "unchanged" => {
                        return Ok(changes);
                    }
                    _ => {}
                }
            }
            if !changes.is_empty() && ok_workloads.len() == 1 {
                trace_step(format!("workloads reached ok for contract {contract_id}"));
                return Ok(changes);
            }
            if changes.is_empty() {
                if let Some(workloads) = self
                    .workloads_from_get(node_twin_id, contract_id, deadline)
                    .await?
                {
                    return Ok(workloads);
                }
            }
            if Instant::now() >= deadline {
                return Err(GridError::backend(format!(
                    "timed out waiting for deployment changes for contract {contract_id}"
                )));
            }
            sleep(Duration::from_secs(2)).await;
        }
    }

    async fn deploy_and_confirm(
        &self,
        node_twin_id: u32,
        deployment: &DeployDeployment,
    ) -> Result<(), GridError> {
        for attempt in 0..10 {
            trace_step(format!(
                "deploy attempt {} for contract {} to twin {}",
                attempt + 1,
                deployment.contract_id,
                node_twin_id
            ));
            let deploy_result = self
                .rmb_call::<_, ()>(node_twin_id, "zos.deployment.deploy", deployment, None)
                .await;
            match deploy_result {
                Ok(()) => {}
                Err(GridError::Backend(message)) if message.contains("exists") => {
                    trace_step(format!(
                        "deployment {} already exists on node twin {}",
                        deployment.contract_id, node_twin_id
                    ));
                    return Ok(());
                }
                Err(err) => {
                    trace_step(format!(
                        "deploy call failed for contract {} on attempt {}: {}",
                        deployment.contract_id,
                        attempt + 1,
                        err
                    ));
                }
            }

            sleep(Duration::from_secs(2)).await;
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                match self
                    .rmb_call::<_, serde_json::Value>(
                        node_twin_id,
                        "zos.deployment.get",
                        &json!({ "contract_id": deployment.contract_id }),
                        None,
                    )
                    .await
                {
                    Ok(_) => {
                        trace_step(format!(
                            "deployment {} appeared on node twin {}",
                            deployment.contract_id, node_twin_id
                        ));
                        return Ok(());
                    }
                    Err(GridError::Backend(message))
                        if message.contains("deployment not found")
                            || message.contains("contract not found")
                            || message
                                .contains("relay closed before zos.deployment.get response")
                            || message.contains("rmb timeout for zos.deployment.get") =>
                    {
                        if Instant::now() >= deadline {
                            trace_step(format!(
                                "deployment {} still missing after attempt {}",
                                deployment.contract_id,
                                attempt + 1
                            ));
                            break;
                        }
                        sleep(Duration::from_secs(2)).await;
                    }
                    Err(err) => return Err(err),
                }
            }
        }
        Err(GridError::backend(format!(
            "deployment {} did not appear on node",
            deployment.contract_id
        )))
    }

    async fn workloads_from_get(
        &self,
        node_twin_id: u32,
        contract_id: u64,
        deadline: Instant,
    ) -> Result<Option<Vec<zos::Workload>>, GridError> {
        match self
            .rmb_call::<_, serde_json::Value>(
                node_twin_id,
                "zos.deployment.get",
                &json!({ "contract_id": contract_id }),
                None,
            )
            .await
        {
            Ok(deployment) => {
                let workloads = extract_workloads(deployment)?;
                if !workloads.is_empty() {
                    let states = workloads
                        .iter()
                        .map(|workload| format!("{}={}", workload.name, workload.result.state))
                        .collect::<Vec<_>>()
                        .join(", ");
                    trace_step(format!(
                        "deployment.get states for contract {contract_id}: {states}"
                    ));
                }
                let mut ok_workloads: HashMap<String, bool> = HashMap::new();
                for workload in &workloads {
                    match workload.result.state.as_str() {
                        "ok" => {
                            ok_workloads.insert(workload.name.clone(), true);
                        }
                        "error" | "deleted" | "paused" | "unchanged" => {
                            return Ok(Some(workloads));
                        }
                        _ => {}
                    }
                }
                if !workloads.is_empty() && ok_workloads.len() == 1 {
                    trace_step(format!(
                        "deployment.get confirmed ok workloads for contract {contract_id}"
                    ));
                    return Ok(Some(workloads));
                }
                Ok(None)
            }
            Err(GridError::Backend(message))
                if message.contains("deployment not found")
                    || message.contains("contract not found")
                    || message.contains("relay closed before zos.deployment.get response")
                    || message.contains("rmb timeout for zos.deployment.get") =>
            {
                trace_step(format!(
                    "deployment.get retry for contract {contract_id}: {message}"
                ));
                if Instant::now() >= deadline {
                    return Err(GridError::backend(format!(
                        "timed out waiting for deployment get for contract {contract_id}"
                    )));
                }
                Ok(None)
            }
            Err(err) => Err(err),
        }
    }

    async fn rmb_call<T, R>(
        &self,
        destination_twin_id: u32,
        command: &str,
        payload: &T,
        session: Option<&str>,
    ) -> Result<R, GridError>
    where
        T: Serialize,
        R: for<'de> Deserialize<'de> + 'static,
    {
        let destination_twin =
            fetch_twin(&self.http, &self.grid_proxy_url, destination_twin_id).await?;
        let relay_hosts = vec![relay_host(&self.relay_url)?];
        let now = now_unix()?;
        let ttl = self.rmb_timeout.as_secs();
        let uid = random_uid();
        let body = serde_json::to_vec(payload).map_err(GridError::from)?;
        let mut envelope = RmbEnvelope {
            uid: uid.clone(),
            tags: None,
            timestamp: now,
            expiration: ttl,
            source: Some(RmbAddress {
                twin: self.identity.twin_id,
                connection: Some(self.session_id.clone()),
            }),
            destination: Some(RmbAddress {
                twin: destination_twin_id,
                connection: session.map(ToOwned::to_owned),
            }),
            message: Some(rmb_envelope::Message::Request(RmbRequest {
                command: command.to_string(),
            })),
            signature: None,
            schema: Some(RMB_SCHEMA.to_string()),
            federation: destination_twin.relay.clone(),
            payload: Some(rmb_envelope::Payload::Plain(body)),
            relays: relay_hosts,
        };
        let challenge = envelope_challenge(&envelope)?;
        let signature = substrate_sign(&self.signer, &challenge);
        let mut typed_sig = vec![b's'];
        typed_sig.extend_from_slice(signature.as_ref());
        envelope.signature = Some(typed_sig);

        let mut socket_guard = self.relay_socket.lock().await;
        self.ensure_relay_socket(&mut socket_guard).await?;
        let send_result = {
            let socket = socket_guard
                .as_mut()
                .ok_or_else(|| GridError::backend("relay socket missing after connect"))?;
            socket
                .send(WsMessage::Binary(envelope.encode_to_vec()))
                .await
        };
        if let Err(err) = send_result {
            *socket_guard = None;
            return Err(GridError::backend(err.to_string()));
        }

        let deadline = Instant::now() + Duration::from_secs(ttl);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let message = {
                let socket = socket_guard
                    .as_mut()
                    .ok_or_else(|| GridError::backend("relay socket missing while waiting"))?;
                tokio::time::timeout(remaining, socket.next()).await
            };
            let message = match message {
                Ok(message) => message,
                Err(_) => {
                    *socket_guard = None;
                    return Err(GridError::backend(format!("rmb timeout for {command}")));
                }
            };
            let Some(message) = message else {
                *socket_guard = None;
                break;
            };
            let message = match message {
                Ok(message) => message,
                Err(err) => {
                    *socket_guard = None;
                    return Err(GridError::backend(err.to_string()));
                }
            };
            let WsMessage::Binary(frame) = message else {
                continue;
            };
            let response = RmbEnvelope::decode(frame.as_slice())
                .map_err(|err| GridError::backend(err.to_string()))?;
            if response.uid != uid {
                continue;
            }
            if let Some(rmb_envelope::Message::Error(error)) = response.message {
                return Err(GridError::backend(error.message));
            }
            let payload = response.payload;
            if TypeId::of::<R>() == TypeId::of::<()>() {
                return serde_json::from_value(serde_json::Value::Null).map_err(GridError::from);
            }
            let payload = match payload {
                Some(rmb_envelope::Payload::Plain(bytes)) => bytes,
                Some(rmb_envelope::Payload::Cipher(bytes)) => {
                    let source_twin_id = response
                        .source
                        .as_ref()
                        .map(|source| source.twin)
                        .unwrap_or(destination_twin_id);
                    trace_step(format!(
                        "decrypting cipher payload from twin {source_twin_id} for {command}"
                    ));
                    let source_twin =
                        fetch_twin(&self.http, &self.grid_proxy_url, source_twin_id).await?;
                    decrypt_rmb_payload(&self.e2e_private_key, &source_twin, &bytes)?
                }
                None => {
                    if Instant::now() >= deadline {
                        return Err(GridError::backend("rmb response missing payload"));
                    }
                    continue;
                }
            };
            return serde_json::from_slice(&payload).map_err(GridError::from);
        }
        if TypeId::of::<R>() == TypeId::of::<()>() {
            return serde_json::from_value(serde_json::Value::Null).map_err(GridError::from);
        }
        if Instant::now() >= deadline {
            return Err(GridError::backend(format!("rmb timeout for {command}")));
        }
        Err(GridError::backend(format!(
            "relay closed before {command} response"
        )))
    }

    async fn ensure_relay_socket(&self, socket: &mut Option<RelaySocket>) -> Result<(), GridError> {
        if socket.is_some() {
            return Ok(());
        }
        let token = jwt_token(
            &self.signer,
            self.identity.twin_id,
            Some(&self.session_id),
            60,
        )?;
        let relay_url = relay_ws_url(&self.relay_url, &token)?;
        let (connected, _) = connect_async(relay_url.as_str())
            .await
            .map_err(|err| GridError::backend(err.to_string()))?;
        *socket = Some(connected);
        Ok(())
    }
}

fn e2e_private_key_from_mnemonic(
    mnemonic: &subxt_signer::bip39::Mnemonic,
) -> Result<SecpSecretKey, GridError> {
    let (entropy, len) = mnemonic.to_entropy_array();
    let mut seed = [0u8; 64];
    pbkdf2::<Hmac<Sha512>>(&entropy[..len], b"mnemonic", 2048, &mut seed)
        .map_err(|err| GridError::backend(err.to_string()))?;
    SecpSecretKey::from_slice(&seed[..32]).map_err(|err| GridError::backend(err.to_string()))
}

fn decrypt_rmb_payload(
    e2e_private_key: &SecpSecretKey,
    source_twin: &ProxyTwin,
    cipher_text: &[u8],
) -> Result<Vec<u8>, GridError> {
    let public_key_hex = source_twin.public_key.as_deref().ok_or_else(|| {
        GridError::backend(format!("twin {} missing public key", source_twin.twin_id))
    })?;
    let public_key_hex = public_key_hex.trim_start_matches("0x");
    let public_key_bytes =
        hex::decode(public_key_hex).map_err(|err| GridError::backend(err.to_string()))?;
    let public_key = SecpPublicKey::from_slice(&public_key_bytes)
        .map_err(|err| GridError::backend(err.to_string()))?;
    let shared_secret = shared_secret_point(&public_key, e2e_private_key);
    let key = Sha256::digest(&shared_secret[..32]);
    let cipher = Aes256Gcm::new_from_slice(key.as_slice())
        .map_err(|err| GridError::backend(format!("init RMB cipher: {err}")))?;
    let nonce_size = 12;
    if cipher_text.len() < nonce_size {
        return Err(GridError::backend("invalid encrypted RMB payload"));
    }
    let (nonce_bytes, encrypted) = cipher_text.split_at(nonce_size);
    cipher
        .decrypt(Nonce::from_slice(nonce_bytes), encrypted)
        .map_err(|err| GridError::backend(format!("decrypt RMB payload: {err}")))
}

fn extract_workloads(payload: serde_json::Value) -> Result<Vec<zos::Workload>, GridError> {
    let workloads = match payload {
        serde_json::Value::Array(items) => items,
        serde_json::Value::Object(map) => map
            .get("workloads")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let normalized = workloads
        .into_iter()
        .map(normalize_workload)
        .collect::<Vec<_>>();
    serde_json::from_value(serde_json::Value::Array(normalized)).map_err(GridError::from)
}

fn normalize_workload(workload: serde_json::Value) -> serde_json::Value {
    let mut workload = workload;
    let Some(workload_obj) = workload.as_object_mut() else {
        return workload;
    };
    let result = workload_obj
        .entry("result")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    let Some(result_obj) = result.as_object_mut() else {
        return workload;
    };
    result_obj
        .entry("created")
        .or_insert_with(|| serde_json::Value::Number(0u64.into()));
    result_obj
        .entry("state")
        .or_insert_with(|| serde_json::Value::String(String::new()));
    result_obj
        .entry("error")
        .or_insert_with(|| serde_json::Value::String(String::new()));
    result_obj.entry("data").or_insert(serde_json::Value::Null);
    workload
}

fn derived_names(request: &VmLightDeployment, suffix: u64) -> DerivedNames {
    let vm_name = request
        .vm
        .name
        .clone()
        .unwrap_or_else(|| format!("rust_vm_light_{suffix}"));
    DerivedNames { vm_name }
}

fn vm_ip_from_subnet(subnet: &str) -> Result<String, GridError> {
    let base = subnet
        .split_once('/')
        .map(|(ip, _)| ip)
        .ok_or_else(|| GridError::validation(format!("invalid subnet: {subnet}")))?;
    let mut octets = base.split('.');
    let first = octets
        .next()
        .ok_or_else(|| GridError::validation(format!("invalid subnet: {subnet}")))?;
    let second = octets
        .next()
        .ok_or_else(|| GridError::validation(format!("invalid subnet: {subnet}")))?;
    let third = octets
        .next()
        .ok_or_else(|| GridError::validation(format!("invalid subnet: {subnet}")))?;
    let _fourth = octets
        .next()
        .ok_or_else(|| GridError::validation(format!("invalid subnet: {subnet}")))?;
    if octets.next().is_some() {
        return Err(GridError::validation(format!("invalid subnet: {subnet}")));
    }
    Ok(format!("{first}.{second}.{third}.5"))
}

fn generate_wireguard_private_key() -> String {
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    key[0] &= 248;
    key[31] &= 127;
    key[31] |= 64;
    base64::engine::general_purpose::STANDARD.encode(key)
}

fn resolve_public_ips_from_workloads(
    workloads: &[zos::Workload],
    workload_name: &str,
) -> (String, String) {
    let public_ip_workload_name = format!("{workload_name}ip");
    let Some(wl) = workloads
        .iter()
        .find(|candidate| candidate.name == public_ip_workload_name)
    else {
        return (String::new(), String::new());
    };
    if !wl.result.is_okay() {
        return (String::new(), String::new());
    }
    let data = wl.result.data.clone();
    let ipv4 = extract_ip_value(data.get("ip")).unwrap_or_default();
    let ipv6 = extract_ip_value(data.get("ipv6"))
        .or_else(|| extract_ip_value(data.get("ip6")))
        .unwrap_or_default();
    (ipv4, ipv6)
}

fn extract_ip_value(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::String(ip) if !ip.is_empty() => Some(ip.clone()),
        serde_json::Value::Object(map) => map
            .get("ip")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
        _ => None,
    }
}

async fn submit_create_node_contract(
    chain: &OnlineClient<PolkadotConfig>,
    signer: &Keypair,
    node_id: u32,
    deployment_data: &str,
    deployment_hash_hex: &str,
    public_ips: u32,
) -> Result<(), GridError> {
    let mut hash_bytes = [0u8; 32];
    hash_bytes.copy_from_slice(deployment_hash_hex.as_bytes());
    let tx = dynamic::tx(
        "SmartContractModule",
        "create_node_contract",
        vec![
            Value::u128(u128::from(node_id)),
            Value::from_bytes(hash_bytes),
            Value::from_bytes(deployment_data.as_bytes()),
            Value::u128(u128::from(public_ips)),
            Value::unnamed_variant("None", []),
        ],
    );
    let progress = chain
        .tx()
        .sign_and_submit_then_watch_default(&tx, signer)
        .await
        .map_err(|err| GridError::backend(err.to_string()))?;
    wait_for_finalized(progress).await
}

async fn submit_update_twin_relay(
    chain: &OnlineClient<PolkadotConfig>,
    signer: &Keypair,
    relay: &str,
) -> Result<(), GridError> {
    let tx = dynamic::tx(
        "TfgridModule",
        "update_twin",
        vec![
            Value::unnamed_variant("Some", [Value::string(relay)]),
            Value::unnamed_variant("None", []),
        ],
    );
    let progress = chain
        .tx()
        .sign_and_submit_then_watch_default(&tx, signer)
        .await
        .map_err(|err| GridError::backend(err.to_string()))?;
    wait_for_finalized(progress).await
}

async fn submit_cancel_contract(
    chain: &OnlineClient<PolkadotConfig>,
    signer: &Keypair,
    contract_id: u64,
) -> Result<(), GridError> {
    let tx = dynamic::tx(
        "SmartContractModule",
        "cancel_contract",
        vec![Value::u128(u128::from(contract_id))],
    );
    let progress = chain
        .tx()
        .sign_and_submit_then_watch_default(&tx, signer)
        .await
        .map_err(|err| GridError::backend(err.to_string()))?;
    wait_for_finalized(progress).await
}

async fn wait_for_finalized(
    progress: TxProgress<PolkadotConfig, OnlineClient<PolkadotConfig>>,
) -> Result<(), GridError> {
    progress
        .wait_for_finalized_success()
        .await
        .map(|_| ())
        .map_err(|err| GridError::backend(err.to_string()))
}

async fn fetch_own_twin_id(
    http: &reqwest::Client,
    grid_proxy_url: &str,
    account_id: &str,
) -> Result<u32, GridError> {
    let response = http
        .get(format!("{grid_proxy_url}/twins"))
        .query(&[("account_id", account_id)])
        .send()
        .await
        .map_err(|err| GridError::backend(err.to_string()))?;
    let twins: Vec<ProxyTwin> =
        parse_json_response(response, "grid proxy /twins account lookup").await?;
    twins
        .into_iter()
        .next()
        .map(|twin| twin.twin_id)
        .ok_or_else(|| GridError::NotFound(format!("twin for account {account_id}")))
}

async fn fetch_twin(
    http: &reqwest::Client,
    grid_proxy_url: &str,
    twin_id: u32,
) -> Result<ProxyTwin, GridError> {
    let response = http
        .get(format!("{grid_proxy_url}/twins"))
        .query(&[("twin_id", twin_id.to_string())])
        .send()
        .await
        .map_err(|err| GridError::backend(err.to_string()))?;
    let twins: Vec<ProxyTwin> =
        parse_json_response(response, "grid proxy /twins twin lookup").await?;
    twins
        .into_iter()
        .next()
        .ok_or_else(|| GridError::NotFound(format!("twin {twin_id}")))
}

fn deployment_metadata(name: &str, kind: &str, project_name: &str) -> String {
    serde_json::to_string(&DeploymentMetadata {
        version: workloads::VERSION4,
        kind,
        name,
        project_name,
    })
    .unwrap_or_default()
}

fn has_feature(node: &ProxyNode, feature: &str) -> bool {
    node.features.iter().any(|item| item == feature)
}

fn free_mru(node: &ProxyNode) -> u64 {
    node.total_resources
        .mru
        .saturating_sub(node.used_resources.mru)
}

fn free_sru(node: &ProxyNode) -> u64 {
    node.total_resources
        .sru
        .saturating_sub(node.used_resources.sru)
}

fn node_has_usable_public_ipv4(node: &ProxyNode) -> bool {
    !node.public_config.ipv4.trim().is_empty()
        && !node.public_config.gw4.trim().is_empty()
        && !node.public_config.domain.trim().is_empty()
}

fn node_has_usable_public_ipv6(node: &ProxyNode) -> bool {
    !node.public_config.ipv6.trim().is_empty() && !node.public_config.gw6.trim().is_empty()
}

fn relay_host(relay_url: &str) -> Result<String, GridError> {
    Url::parse(relay_url)
        .ok()
        .and_then(|url| url.host_str().map(ToOwned::to_owned))
        .ok_or_else(|| GridError::validation(format!("invalid relay url {relay_url}")))
}

fn relay_ws_url(base: &str, token: &str) -> Result<Url, GridError> {
    let mut url = Url::parse(base)
        .map_err(|err| GridError::validation(format!("invalid relay url {base}: {err}")))?;
    if url.path().is_empty() {
        url.set_path("/");
    }
    url.set_query(Some(token));
    Ok(url)
}

fn envelope_challenge(envelope: &RmbEnvelope) -> Result<Vec<u8>, GridError> {
    let mut challenge = String::new();
    challenge.push_str(&envelope.uid);
    if let Some(tags) = &envelope.tags {
        challenge.push_str(tags);
    }
    write!(&mut challenge, "{}", envelope.timestamp)
        .map_err(|err| GridError::backend(err.to_string()))?;
    write!(&mut challenge, "{}", envelope.expiration)
        .map_err(|err| GridError::backend(err.to_string()))?;
    if let Some(source) = &envelope.source {
        write!(&mut challenge, "{}", source.twin)
            .map_err(|err| GridError::backend(err.to_string()))?;
        if let Some(connection) = &source.connection {
            challenge.push_str(connection);
        }
    }
    if let Some(destination) = &envelope.destination {
        write!(&mut challenge, "{}", destination.twin)
            .map_err(|err| GridError::backend(err.to_string()))?;
        if let Some(connection) = &destination.connection {
            challenge.push_str(connection);
        }
    }
    if let Some(message) = &envelope.message {
        match message {
            rmb_envelope::Message::Request(request) => challenge.push_str(&request.command),
            rmb_envelope::Message::Response(_) => {}
            rmb_envelope::Message::Error(error) => {
                write!(&mut challenge, "{}", error.code)
                    .map_err(|err| GridError::backend(err.to_string()))?;
                challenge.push_str(&error.message);
            }
        }
    }
    if let Some(schema) = &envelope.schema {
        challenge.push_str(schema);
    }
    if let Some(federation) = &envelope.federation {
        challenge.push_str(federation);
    }
    if let Some(payload) = &envelope.payload {
        match payload {
            rmb_envelope::Payload::Plain(bytes) | rmb_envelope::Payload::Cipher(bytes) => {
                challenge.push_str(&String::from_utf8_lossy(bytes));
            }
        }
    }
    for relay in &envelope.relays {
        challenge.push_str(relay);
    }
    Ok(md5::compute(challenge.as_bytes()).0.to_vec())
}

fn jwt_token(
    signer: &Keypair,
    twin_id: u32,
    session: Option<&str>,
    ttl_secs: u64,
) -> Result<String, GridError> {
    let header =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"RS512","typ":"JWT"}"#);
    let now = now_unix()?;
    let mut payload = json!({
        "sub": twin_id,
        "iat": now,
        "exp": now + ttl_secs,
    });
    if let Some(session) = session {
        payload["sid"] = serde_json::Value::String(session.to_string());
    }
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&payload).map_err(GridError::from)?);
    let signing_input = format!("{header}.{payload}");
    let signature = substrate_sign(signer, signing_input.as_bytes());
    let mut typed_sig = vec![b's'];
    typed_sig.extend_from_slice(signature.as_ref());
    let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(typed_sig);
    Ok(format!("{signing_input}.{signature}"))
}

fn substrate_sign(signer: &Keypair, input: &[u8]) -> subxt_signer::sr25519::Signature {
    if input.len() > 256 {
        let mut hasher = Blake2b::<U32>::new();
        hasher.update(input);
        signer.sign(&hasher.finalize())
    } else {
        signer.sign(input)
    }
}

fn random_uid() -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 16];
    rng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn random_bytes(len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
}

fn debug_dump(label: &str, deployment: &DeployDeployment, hash: &str) {
    if std::env::var_os("TFGRID_DEBUG").is_none() {
        return;
    }
    eprintln!(
        "{label} deployment hash: {hash}\n{}",
        serde_json::to_string_pretty(deployment).unwrap_or_default()
    );
}

fn trace_step(message: impl AsRef<str>) {
    if std::env::var_os("TFGRID_DEBUG").is_none() {
        return;
    }
    eprintln!("[tfgrid-debug] {}", message.as_ref());
}

#[cfg(test)]
mod tests {
    use super::{
        NodeRequirements, ProxyCapacity, ProxyNode, ProxyPublicConfig, extract_workloads, free_mru,
        free_sru, has_feature, node_has_usable_public_ipv4, normalize_workload, public_ip_count,
    };
    use crate::zos;
    use serde_json::json;

    #[test]
    fn normalize_workload_fills_sparse_result_fields() {
        let workload = json!({
            "version": 0,
            "name": "vm",
            "type": "zmachine-light",
            "data": {},
            "metadata": "",
            "description": "",
            "result": {
                "state": "ok"
            }
        });

        let normalized = normalize_workload(workload);
        let result = &normalized["result"];
        assert_eq!(result["state"], "ok");
        assert_eq!(result["error"], "");
        assert_eq!(result["created"], 0);
        assert_eq!(result["data"], serde_json::Value::Null);
    }

    #[test]
    fn extract_workloads_accepts_deployment_object_payload() {
        let payload = json!({
            "workloads": [{
                "version": 0,
                "name": "net",
                "type": "network-light",
                "data": {},
                "metadata": "",
                "description": "",
                "result": {
                    "state": "ok"
                }
            }]
        });

        let workloads = extract_workloads(payload).expect("workloads from object payload");
        assert_eq!(workloads.len(), 1);
        assert_eq!(workloads[0].name, "net");
        assert_eq!(workloads[0].result.state, zos::STATE_OK);
    }

    #[test]
    fn extract_workloads_accepts_changes_array_payload() {
        let payload = json!([
            {
                "version": 0,
                "name": "vm",
                "type": "zmachine-light",
                "data": {},
                "metadata": "",
                "description": "",
                "result": {
                    "state": "init"
                }
            },
            {
                "version": 0,
                "name": "vm",
                "type": "zmachine-light",
                "data": {},
                "metadata": "",
                "description": "",
                "result": {
                    "state": "ok"
                }
            }
        ]);

        let workloads = extract_workloads(payload).expect("workloads from array payload");
        assert_eq!(workloads.len(), 2);
        assert_eq!(workloads[0].result.state, zos::STATE_INIT);
        assert_eq!(workloads[1].result.state, zos::STATE_OK);
    }

    #[test]
    fn validate_vm_light_request_rejects_zero_cpu() {
        let request = super::VmLightDeployment {
            placement: super::NodePlacement::default(),
            network: super::NetworkTarget::Create(super::NetworkLightSpec::default()),
            vm: super::VmLightSpec {
                cpu: 0,
                ..Default::default()
            },
        };

        let err = super::validate_vm_light_request(&request).expect_err("zero cpu must fail");
        assert!(matches!(err, crate::GridError::Validation(_)));
    }

    #[test]
    fn vm_ip_from_subnet_uses_host_five() {
        let ip = super::vm_ip_from_subnet("10.24.2.0/24").expect("vm ip");
        assert_eq!(ip, "10.24.2.5");
    }

    #[test]
    fn validate_vm_request_rejects_zero_sized_volume() {
        let request = super::VmDeployment {
            placement: super::NodePlacement::default(),
            network: super::FullNetworkTarget::Create(super::FullNetworkSpec::default()),
            vm: super::VmSpec {
                volumes: vec![super::VolumeMountSpec {
                    name: "data".to_string(),
                    size_bytes: 0,
                    mountpoint: "/data".to_string(),
                    description: String::new(),
                }],
                ..Default::default()
            },
        };

        let err = super::validate_vm_request(&request).expect_err("zero-sized volume must fail");
        assert!(matches!(err, crate::GridError::Validation(_)));
    }

    #[test]
    fn public_ipv4_node_requires_usable_public_config() {
        let unusable = proxy_node_with_public_ipv4("", "", "", 5);
        assert!(has_feature(&unusable, "ip"));
        assert!(!node_has_usable_public_ipv4(&unusable));

        let usable = proxy_node_with_public_ipv4(
            "185.206.122.32/24",
            "185.206.122.1",
            "gent02.dev.grid.tf",
            5,
        );
        assert!(node_has_usable_public_ipv4(&usable));
    }

    #[test]
    fn public_ipv4_node_requires_free_ips() {
        let node = proxy_node_with_public_ipv4(
            "185.206.122.32/24",
            "185.206.122.1",
            "gent02.dev.grid.tf",
            0,
        );
        let has_base = node.status == "up"
            && node.healthy
            && has_feature(&node, "network")
            && has_feature(&node, "zmachine")
            && has_feature(&node, "volume")
            && free_mru(&node) >= NodeRequirements::default().min_memory_bytes
            && free_sru(&node) >= NodeRequirements::default().min_rootfs_bytes
            && node
                .total_resources
                .cru
                .saturating_sub(node.used_resources.cru)
                >= NodeRequirements::default().min_cru;
        assert!(has_base);
        assert_eq!(node.farm_free_ips, 0);
    }

    #[test]
    fn public_ip_count_matches_public_ip_workloads() {
        let workloads = vec![
            super::deployment::DeployWorkload {
                version: 0,
                name: "vmip".to_string(),
                workload_type: zos::PUBLIC_IP_TYPE.to_string(),
                data: json!({ "v4": true, "v6": false }),
                metadata: String::new(),
                description: String::new(),
                result: super::deployment::empty_result_data(),
            },
            super::deployment::DeployWorkload {
                version: 0,
                name: "vm".to_string(),
                workload_type: zos::ZMACHINE_TYPE.to_string(),
                data: json!({}),
                metadata: String::new(),
                description: String::new(),
                result: super::deployment::empty_result_data(),
            },
        ];

        assert_eq!(public_ip_count(&workloads), 1);
    }

    #[test]
    fn vm_light_builder_collects_network_and_volume_settings() {
        let request = super::VmLightDeployment::builder()
            .auto_with(
                super::NodeRequirements::builder()
                    .min_cru(2)
                    .min_memory_bytes(2 * 1024 * 1024 * 1024)
                    .min_rootfs_bytes(20 * 1024 * 1024 * 1024)
                    .build(),
            )
            .create_network(
                super::NetworkLightSpec::builder()
                    .name("net")
                    .subnet("10.42.2.0/24")
                    .build(),
            )
            .vm(super::VmLightSpec::builder()
                .name("vm")
                .cpu(2)
                .memory_bytes(2 * 1024 * 1024 * 1024)
                .rootfs_size_bytes(20 * 1024 * 1024 * 1024)
                .env("SSH_KEY", "ssh-ed25519 test")
                .volume(
                    super::VolumeMountSpec::new("data", 5 * 1024 * 1024 * 1024, "/data")
                        .description("data volume"),
                )
                .build())
            .build();

        assert_eq!(request.vm.name.as_deref(), Some("vm"));
        assert_eq!(request.vm.cpu, 2);
        assert_eq!(request.vm.volumes.len(), 1);
        assert_eq!(
            request.vm.env.get("SSH_KEY").map(String::as_str),
            Some("ssh-ed25519 test")
        );
        match request.network {
            super::NetworkTarget::Create(network) => {
                assert_eq!(network.name.as_deref(), Some("net"));
                assert_eq!(network.subnet.as_deref(), Some("10.42.2.0/24"));
            }
            other => panic!("unexpected network target: {other:?}"),
        }
        match request.placement {
            super::NodePlacement::Auto(requirements) => {
                assert_eq!(requirements.min_cru, 2);
            }
            other => panic!("unexpected placement: {other:?}"),
        }
    }

    #[test]
    fn vm_builder_sets_fixed_placement_and_public_networking() {
        let request = super::VmDeployment::builder()
            .fixed_node(11, 21)
            .create_network(
                super::FullNetworkSpec::builder()
                    .name("full-net")
                    .ip_range("10.60.0.0/16")
                    .subnet("10.60.2.0/24")
                    .wireguard_listen_port(1024)
                    .build(),
            )
            .vm(super::VmSpec::builder()
                .name("full-vm")
                .cpu(2)
                .public_ipv4(true)
                .planetary(true)
                .build())
            .build();

        match request.placement {
            super::NodePlacement::Fixed {
                node_id,
                node_twin_id,
            } => {
                assert_eq!(node_id, 11);
                assert_eq!(node_twin_id, 21);
            }
            other => panic!("unexpected placement: {other:?}"),
        }
        match request.network {
            super::FullNetworkTarget::Create(network) => {
                assert_eq!(network.name.as_deref(), Some("full-net"));
                assert_eq!(network.ip_range.as_deref(), Some("10.60.0.0/16"));
            }
            other => panic!("unexpected network target: {other:?}"),
        }
        assert!(request.vm.public_ipv4);
        assert!(request.vm.planetary);
        assert_eq!(request.vm.name.as_deref(), Some("full-vm"));
    }

    #[test]
    fn grid_client_config_builder_overrides_devnet_defaults() {
        let config = super::GridClientConfig::builder()
            .substrate_url("wss://example.test/ws")
            .grid_proxy_url("https://proxy.example.test")
            .relay_url("wss://relay.example.test")
            .http_timeout(std::time::Duration::from_secs(10))
            .rmb_timeout(std::time::Duration::from_secs(15))
            .build();

        assert_eq!(config.substrate_url, "wss://example.test/ws");
        assert_eq!(config.grid_proxy_url, "https://proxy.example.test");
        assert_eq!(config.relay_url, "wss://relay.example.test");
        assert_eq!(config.http_timeout, std::time::Duration::from_secs(10));
        assert_eq!(config.rmb_timeout, std::time::Duration::from_secs(15));
    }

    fn proxy_node_with_public_ipv4(
        ipv4: &str,
        gw4: &str,
        domain: &str,
        farm_free_ips: u64,
    ) -> ProxyNode {
        ProxyNode {
            node_id: 11,
            twin_id: 21,
            status: "up".to_string(),
            healthy: true,
            features: vec![
                "network".to_string(),
                "zmachine".to_string(),
                "volume".to_string(),
                "ip".to_string(),
                "ipv4".to_string(),
            ],
            farm_free_ips,
            public_config: ProxyPublicConfig {
                domain: domain.to_string(),
                gw4: gw4.to_string(),
                gw6: "2a10:b600:1::1".to_string(),
                ipv4: ipv4.to_string(),
                ipv6: "2a10:b600:1::0025:90f0:ede1/64".to_string(),
            },
            total_resources: ProxyCapacity {
                cru: 4,
                mru: 8 * 1024 * 1024 * 1024,
                sru: 100 * 1024 * 1024 * 1024,
            },
            used_resources: ProxyCapacity {
                cru: 0,
                mru: 0,
                sru: 0,
            },
        }
    }
}

fn now_unix() -> Result<u64, GridError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|err| GridError::backend(err.to_string()))
}

async fn parse_json_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
    context: &str,
) -> Result<T, GridError> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| GridError::backend(err.to_string()))?;
    if !status.is_success() {
        return Err(GridError::backend(format!(
            "{context} failed with {status}: {body}"
        )));
    }
    serde_json::from_str(&body).map_err(GridError::from)
}

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        hex::decode(raw).map_err(serde::de::Error::custom)
    }
}
