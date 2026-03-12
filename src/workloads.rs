//! Minimal workload model layer used by state/deployer layers.

use std::collections::HashMap;
use std::net::IpAddr;
use std::str::FromStr;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{error::GridError, zos};

pub fn contains<T: PartialEq>(elements: &[T], element: &T) -> bool {
    elements.iter().any(|value| value == element)
}

pub fn delete<T: PartialEq>(elements: &mut Vec<T>, element: &T) {
    if let Some(idx) = elements.iter().position(|value| value == element) {
        elements.remove(idx);
    }
}

pub fn to_map<T: Serialize>(value: T) -> Result<HashMap<String, serde_json::Value>, GridError> {
    serde_json::from_value(serde_json::to_value(value).map_err(GridError::from)?)
        .map_err(GridError::from)
}

pub fn new_workload_from_map<T: for<'a> Deserialize<'a>>(
    map: &HashMap<String, serde_json::Value>,
) -> Result<T, GridError> {
    serde_json::from_value(serde_json::Value::Object(
        map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
    ))
    .map_err(GridError::from)
}

fn parse_bytes(value: &serde_json::Value) -> Vec<u8> {
    match value {
        serde_json::Value::Array(bytes) => bytes
            .iter()
            .filter_map(|value| value.as_u64())
            .filter_map(|byte| u8::try_from(byte).ok())
            .collect(),
        serde_json::Value::String(raw) => {
            decode_hex_like(raw).unwrap_or_else(|| raw.as_bytes().to_vec())
        }
        _ => Vec::new(),
    }
}

fn decode_hex_like(raw: &str) -> Option<Vec<u8>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Some(Vec::new());
    }
    if raw.len() % 2 != 0 {
        return None;
    }
    let is_hex = raw.as_bytes().iter().all(|b| b.is_ascii_hexdigit());
    if !is_hex {
        return None;
    }

    (0..raw.len())
        .step_by(2)
        .map(|idx| u8::from_str_radix(&raw[idx..idx + 2], 16).ok())
        .collect()
}

fn get_hex_bytes(value: &serde_json::Value) -> Vec<u8> {
    match value {
        serde_json::Value::String(raw) => {
            decode_hex_like(raw).unwrap_or_else(|| raw.as_bytes().to_vec())
        }
        serde_json::Value::Array(_) => parse_bytes(value),
        _ => Vec::new(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PublicIPResult {
    #[serde(rename = "ip", default)]
    ip: serde_json::Value,
    #[serde(rename = "ipv6", default)]
    ipv6: serde_json::Value,
    #[serde(rename = "ip6", default)]
    ip6: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct VMResult {
    #[serde(default)]
    #[serde(rename = "planetary_ip")]
    planetary_ip: String,
    #[serde(default)]
    #[serde(rename = "ygg_ip")]
    ygg_ip: String,
    #[serde(default)]
    #[serde(rename = "mycelium_ip")]
    mycelium_ip: String,
    #[serde(default)]
    #[serde(rename = "console_url")]
    console_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct VMLightResult {
    #[serde(default)]
    #[serde(rename = "mycelium_ip")]
    mycelium_ip: String,
    #[serde(default)]
    #[serde(rename = "console_url")]
    console_url: String,
}

pub const VM_TYPE: &str = "vm";
pub const GATEWAY_NAME_TYPE: &str = "Gateway Name";
pub const GATEWAY_FQDN_TYPE: &str = "Gateway Fqdn";
pub const K8S_TYPE: &str = "kubernetes";
pub const NETWORK_TYPE: &str = "network";

pub const VERSION3: i32 = 3;
pub const VERSION4: i32 = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAccess {
    pub subnet: String,
    #[serde(rename = "private_key")]
    pub private_key: String,
    #[serde(rename = "node_id")]
    pub node_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetaData {
    pub version: i32,
    #[serde(default)]
    pub user_accesses: Vec<UserAccess>,

    #[serde(rename = "UserAccesses", default)]
    user_accesses_deprecated: Vec<UserAccess>,
    #[serde(rename = "Version", default)]
    version_deprecated: Option<i32>,
    #[serde(rename = "UserAccessIP", default)]
    user_access_ip: String,
    #[serde(rename = "PrivateKey", default)]
    private_key: String,
    #[serde(rename = "PublicNodeID", default)]
    public_node_id: u32,
}

impl Default for NetworkMetaData {
    fn default() -> Self {
        Self {
            version: VERSION3,
            user_accesses: Vec::new(),
            user_accesses_deprecated: Vec::new(),
            version_deprecated: None,
            user_access_ip: String::new(),
            private_key: String::new(),
            public_node_id: 0,
        }
    }
}

impl NetworkMetaData {
    pub fn normalized(self) -> Self {
        let mut normalized = self;
        if normalized.user_accesses.is_empty() && !normalized.user_access_ip.is_empty() {
            normalized.user_accesses = vec![UserAccess {
                subnet: normalized.user_access_ip.clone(),
                private_key: normalized.private_key.clone(),
                node_id: normalized.public_node_id,
            }];
        }
        if let Some(version) = normalized.version_deprecated {
            normalized.version = version;
        }
        normalized
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZNet {
    pub name: String,
    pub description: String,
    pub nodes: Vec<u32>,
    pub ip_range: String,
    pub add_wg_access: bool,
    pub mycelium_keys: HashMap<u32, Vec<u8>>,
    pub solution_type: String,
    pub access_wg_config: String,
    pub external_ip: Option<String>,
    pub external_sk: String,
    pub public_node_id: u32,
    pub nodes_ip_range: HashMap<u32, String>,
    pub node_deployment_id: HashMap<u32, u64>,
    pub wg_port: HashMap<u32, i32>,
    pub keys: HashMap<u32, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZNetLight {
    pub name: String,
    pub description: String,
    pub nodes: Vec<u32>,
    pub solution_type: String,
    pub ip_range: String,
    pub nodes_ip_range: HashMap<u32, String>,
    pub node_deployment_id: HashMap<u32, u64>,
    pub public_node_id: u32,
    pub mycelium_keys: HashMap<u32, Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sNode {
    pub vm: VM,
    pub disk_size_gb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sCluster {
    pub master: Option<K8sNode>,
    pub workers: Vec<K8sNode>,
    pub token: String,
    pub network_name: String,
    pub flist: String,
    pub flist_checksum: String,
    pub entry_point: String,
    pub solution_type: String,
    pub ssh_key: String,
    pub nodes_ip_range: HashMap<u32, String>,
    pub node_deployment_id: HashMap<u32, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentData {
    pub version: i32,
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    #[serde(rename = "projectName")]
    pub project_name: String,
}

pub fn parse_deployment_data(meta: &str) -> Result<DeploymentData, GridError> {
    serde_json::from_str(meta).map_err(GridError::from)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NetworkData {
    #[serde(rename = "ip_range")]
    ip_range: String,
    subnet: String,
    #[serde(default, rename = "wireguard_private_key")]
    wg_private_key: String,
    #[serde(default, rename = "wireguard_listen_port")]
    wg_listen_port: u16,
    #[serde(default)]
    mycelium: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NetworkLightData {
    subnet: String,
    #[serde(default)]
    mycelium: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct K8sMachineNetworkData {
    #[serde(default, rename = "public_ip")]
    public_ip: String,
    #[serde(default)]
    planetary: bool,
    #[serde(default)]
    mycelium: Option<serde_json::Value>,
    #[serde(default)]
    interfaces: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct K8sMachineCapacity {
    cpu: u8,
    memory: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct K8sMachineData {
    flist: String,
    #[serde(default)]
    network: K8sMachineNetworkData,
    #[serde(default)]
    compute_capacity: K8sMachineCapacity,
    #[serde(default)]
    #[serde(rename = "compute_cpu")]
    compute_cpu: u8,
    #[serde(default)]
    #[serde(rename = "compute_memory_mb")]
    compute_memory_mb: u64,
    #[serde(default, rename = "rootfs_size_mb")]
    rootfs_size_mb: u64,
    #[serde(default)]
    mounts: Vec<Mount>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    entrypoint: String,
    #[serde(default)]
    corex: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    pub name: String,
    pub node_id: u32,
    pub solution_type: String,
    pub solution_provider: Option<u64>,
    pub network_name: String,
    pub disks: Vec<Disk>,
    pub zdbs: Vec<ZDB>,
    pub vms: Vec<VM>,
    pub vms_light: Vec<VMLight>,
    pub qsfs: Vec<QSFS>,
    pub volumes: Vec<Volume>,
    pub node_deployment_id: HashMap<u32, u64>,
    pub contract_id: u64,
    pub ip_range: String,
}

impl Deployment {
    pub fn new(
        name: &str,
        node_id: u32,
        solution_type: &str,
        solution_provider: Option<u64>,
        network_name: &str,
        disks: Vec<Disk>,
        zdbs: Vec<ZDB>,
        vms: Vec<VM>,
        vms_light: Vec<VMLight>,
        qsfs: Vec<QSFS>,
        volumes: Vec<Volume>,
    ) -> Self {
        Self {
            name: name.to_string(),
            node_id,
            solution_type: solution_type.to_string(),
            solution_provider,
            network_name: network_name.to_string(),
            disks,
            zdbs,
            vms,
            vms_light,
            qsfs,
            volumes,
            node_deployment_id: HashMap::new(),
            contract_id: 0,
            ip_range: String::new(),
        }
    }

    pub fn validate(&self) -> Result<(), GridError> {
        validate_name(&self.name)?;
        if self.node_id == 0 {
            return Err(GridError::validation("node id must be >0"));
        }
        if !self.vms.is_empty() && !self.vms_light.is_empty() {
            return Err(GridError::validation(
                "cannot mix vm and vm-light in one deployment",
            ));
        }
        for vm in &self.vms {
            vm.validate()?;
        }
        for vm in &self.vms_light {
            vm.validate()?;
        }
        for zdb in &self.zdbs {
            zdb.validate()?;
        }
        for q in &self.qsfs {
            q.validate()?;
        }
        for d in &self.disks {
            d.validate()?;
        }
        for v in &self.volumes {
            v.validate()?;
        }
        Ok(())
    }

    pub fn generate_metadata(&self) -> String {
        let data = DeploymentData {
            version: VERSION3,
            kind: if self.vms_light.is_empty() {
                "vm".to_string()
            } else {
                "vm-light".to_string()
            },
            name: self.name.clone(),
            project_name: if self.solution_type.is_empty() {
                self.name.clone()
            } else {
                self.solution_type.clone()
            },
        };
        serde_json::to_string(&data).unwrap_or_default()
    }

    pub fn zos_deployment(&self, twin_id: u32) -> Result<zos::Deployment, GridError> {
        let workloads: Vec<zos::Workload> = self
            .disks
            .iter()
            .map(Disk::zos_workload)
            .chain(self.volumes.iter().map(Volume::zos_workload))
            .chain(self.zdbs.iter().map(ZDB::zos_workload))
            .chain(self.vms.iter().flat_map(VM::zos_workload))
            .chain(self.vms_light.iter().flat_map(VMLight::zos_workload))
            .chain(
                self.qsfs
                    .iter()
                    .map(|q| q.zos_workload())
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter(),
            )
            .collect();

        Ok(zos::Deployment {
            version: 0,
            twin_id,
            contract_id: self.contract_id,
            metadata: self.generate_metadata(),
            description: String::new(),
            expiration: 0,
            signature_requirement: zos::SignatureRequirement {
                requests: vec![zos::SignatureRequest {
                    twin_id,
                    required: true,
                    weight: 1,
                }],
                weight_required: 1,
                signatures: vec![],
                signature_style: String::new(),
            },
            workloads,
        })
    }

    pub fn match_deployments(
        &mut self,
        disks: &[Disk],
        qsfs: &[QSFS],
        zdbs: &[ZDB],
        vms: &[VM],
        vms_light: &[VMLight],
        volumes: &[Volume],
    ) {
        let mut order = HashMap::new();
        for (idx, d) in self.disks.iter().enumerate() {
            order.insert(d.name.clone(), idx as isize);
        }
        for (idx, d) in self.volumes.iter().enumerate() {
            order.insert(d.name.clone(), idx as isize + self.disks.len() as isize);
        }
        for (idx, d) in self.qsfs.iter().enumerate() {
            order.insert(
                d.name.clone(),
                idx as isize + self.disks.len() as isize + self.volumes.len() as isize,
            );
        }
        for (idx, d) in self.zdbs.iter().enumerate() {
            order.insert(
                d.name.clone(),
                idx as isize
                    + self.disks.len() as isize
                    + self.volumes.len() as isize
                    + self.qsfs.len() as isize,
            );
        }
        for (idx, d) in self.vms.iter().enumerate() {
            order.insert(d.name.clone(), idx as isize + 1000);
        }
        for (idx, d) in self.vms_light.iter().enumerate() {
            order.insert(d.name.clone(), idx as isize + 2000);
        }
        let mut all = disks.to_vec();
        all.sort_by_key(|x| *order.get(&x.name).unwrap_or(&10000));
        self.disks = all;

        let mut all_q = qsfs.to_vec();
        all_q.sort_by_key(|x| *order.get(&x.name).unwrap_or(&10000));
        self.qsfs = all_q;

        let mut all_z = zdbs.to_vec();
        all_z.sort_by_key(|x| *order.get(&x.name).unwrap_or(&10000));
        self.zdbs = all_z;

        let mut all_v = vms.to_vec();
        all_v.sort_by_key(|x| *order.get(&x.name).unwrap_or(&10000));
        self.vms = all_v;

        let mut all_vl = vms_light.to_vec();
        all_vl.sort_by_key(|x| *order.get(&x.name).unwrap_or(&10000));
        self.vms_light = all_vl;

        let mut all_vol = volumes.to_vec();
        all_vol.sort_by_key(|x| *order.get(&x.name).unwrap_or(&10000));
        self.volumes = all_vol;
    }

    pub fn nullify(&mut self) {
        self.vms.clear();
        self.vms_light.clear();
        self.qsfs.clear();
        self.disks.clear();
        self.zdbs.clear();
        self.volumes.clear();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mount {
    pub name: String,
    pub mount_point: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VM {
    pub name: String,
    pub node_id: u32,
    pub network_name: String,
    pub description: String,
    pub flist: String,
    pub flist_checksum: String,
    pub entrypoint: String,
    pub public_ip: bool,
    pub public_ip6: bool,
    pub planetary: bool,
    pub corex: bool,
    pub ip: String,
    #[serde(default)]
    pub mycelium_ip_seed: Vec<u8>,
    pub cpus: u8,
    pub memory_mb: u64,
    pub rootfs_size_mb: u64,
    pub mounts: Vec<Mount>,
    pub zlogs: Vec<String>,
    pub env_vars: HashMap<String, String>,
    pub computed_ip: String,
    pub computed_ip6: String,
    pub planetary_ip: String,
    pub mycelium_ip: String,
    pub console_url: String,
}

impl VM {
    pub fn validate(&self) -> Result<(), GridError> {
        validate_name(&self.name)?;
        validate_name(&self.network_name)?;
        if self.node_id == 0 {
            return Err(GridError::validation("node id should be >0"));
        }
        if self.cpus == 0 {
            return Err(GridError::validation("cpu should be greater than zero"));
        }
        if self.memory_mb < 250 {
            return Err(GridError::validation("memory should be at least 250 MB"));
        }
        Ok(())
    }

    pub fn min_root_size(&self) -> u64 {
        let sru = (self.cpus as u64 * self.memory_mb) / (8 * zos::GIGABYTE);
        if sru == 0 {
            500 * zos::MEGABYTE
        } else {
            2 * zos::GIGABYTE
        }
    }

    pub fn zos_workload(&self) -> Vec<zos::Workload> {
        let mut workloads = vec![];
        if self.public_ip || self.public_ip6 {
            workloads.push(construct_public_ip_workload(
                &format!("{}ip", self.name),
                self.public_ip,
                self.public_ip6,
            ));
        }

        let data = VMWorkloadData {
            flist: self.flist.clone(),
            network_name: self.network_name.clone(),
            ip: self.ip.clone(),
            compute_cpu: self.cpus,
            compute_memory_mb: self.memory_mb,
            rootfs_size_mb: self.rootfs_size_mb,
            mounts: self.mounts.iter().cloned().collect(),
            env_vars: self.env_vars.clone(),
            entrypoint: self.entrypoint.clone(),
            corex: self.corex,
        };
        workloads.push(zos::Workload {
            version: 0,
            name: self.name.clone(),
            workload_type: zos::ZMACHINE_TYPE.to_string(),
            data: serde_json::to_value(&data).unwrap_or_default(),
            metadata: String::new(),
            description: self.description.clone(),
            result: zos::ResultData::default(),
        });

        workloads
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VMWorkloadData {
    flist: String,
    network_name: String,
    ip: String,
    compute_cpu: u8,
    compute_memory_mb: u64,
    rootfs_size_mb: u64,
    #[serde(default)]
    mounts: Vec<Mount>,
    #[serde(default)]
    env_vars: HashMap<String, String>,
    entrypoint: String,
    #[serde(default)]
    corex: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMLight {
    pub name: String,
    pub node_id: u32,
    pub network_name: String,
    pub description: String,
    pub flist: String,
    pub flist_checksum: String,
    pub entrypoint: String,
    pub corex: bool,
    pub ip: String,
    #[serde(default)]
    pub mycelium_ip_seed: Vec<u8>,
    pub cpus: u8,
    pub memory_mb: u64,
    pub rootfs_size_mb: u64,
    pub mounts: Vec<Mount>,
    pub zlogs: Vec<String>,
    pub env_vars: HashMap<String, String>,
    pub mycelium_ip: String,
    pub console_url: String,
    pub computed_ip: String,
    pub computed_ip6: String,
}

impl VMLight {
    pub fn validate(&self) -> Result<(), GridError> {
        validate_name(&self.name)?;
        validate_name(&self.network_name)?;
        if self.node_id == 0 {
            return Err(GridError::validation("node id should be >0"));
        }
        if self.cpus == 0 {
            return Err(GridError::validation("cpu should be greater than zero"));
        }
        if self.memory_mb < 250 {
            return Err(GridError::validation("memory should be at least 250 MB"));
        }
        Ok(())
    }

    pub fn min_root_size(&self) -> u64 {
        let sru = (self.cpus as u64 * self.memory_mb) / (8 * zos::GIGABYTE);
        if sru == 0 {
            500 * zos::MEGABYTE
        } else {
            2 * zos::GIGABYTE
        }
    }

    pub fn zos_workload(&self) -> Vec<zos::Workload> {
        let data = VMLightWorkloadData {
            flist: self.flist.clone(),
            network_name: self.network_name.clone(),
            ip: self.ip.clone(),
            compute_cpu: self.cpus,
            compute_memory_mb: self.memory_mb,
            rootfs_size_mb: self.rootfs_size_mb,
            mounts: self.mounts.iter().cloned().collect(),
            env_vars: self.env_vars.clone(),
            entrypoint: self.entrypoint.clone(),
            corex: self.corex,
        };
        vec![zos::Workload {
            version: 0,
            name: self.name.clone(),
            workload_type: zos::ZMACHINE_LIGHT_TYPE.to_string(),
            data: serde_json::to_value(&data).unwrap_or_default(),
            metadata: String::new(),
            description: self.description.clone(),
            result: zos::ResultData::default(),
        }]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VMLightWorkloadData {
    flist: String,
    network_name: String,
    ip: String,
    compute_cpu: u8,
    compute_memory_mb: u64,
    rootfs_size_mb: u64,
    #[serde(default)]
    mounts: Vec<Mount>,
    #[serde(default)]
    env_vars: HashMap<String, String>,
    entrypoint: String,
    #[serde(default)]
    corex: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Disk {
    pub name: String,
    pub size_gb: u64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub name: String,
    pub size_gb: u64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZDB {
    pub name: String,
    pub password: String,
    pub public: bool,
    pub size_gb: u64,
    pub description: String,
    pub mode: String,
    pub ips: Vec<String>,
    pub port: u32,
    pub namespace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QSFS {
    pub name: String,
    pub description: String,
    pub cache: i32,
    pub minimal_shards: u32,
    pub expected_shards: u32,
    pub redundant_groups: u32,
    pub redundant_nodes: u32,
    pub max_zdb_data_dir_size: u32,
    pub encryption_algorithm: String,
    pub encryption_key: String,
    pub compression_algorithm: String,
    pub metadata: String,
    pub groups: serde_json::Value,
    pub metrics_endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayNameProxy {
    pub node_id: u32,
    pub name: String,
    #[serde(default)]
    pub backends: Vec<String>,
    pub tls_passthrough: bool,
    pub network: String,
    pub description: String,
    pub solution_type: String,
    pub node_deployment_id: HashMap<u32, u64>,
    pub fqdn: String,
    pub name_contract_id: u64,
    pub contract_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayFQDNProxy {
    pub node_id: u32,
    #[serde(default)]
    pub backends: Vec<String>,
    pub fqdn: String,
    pub name: String,
    pub tls_passthrough: bool,
    pub network: String,
    pub description: String,
    pub solution_type: String,
    pub contract_id: u64,
    pub node_deployment_id: HashMap<u32, u64>,
}

impl GatewayNameProxy {
    pub fn from_workload(wl: &zos::Workload) -> Result<Self, GridError> {
        let data: serde_json::Value = wl.workload_data::<serde_json::Value>()?;
        let result: serde_json::Value = wl.result.data.clone();
        let name = data
            .get("name")
            .or_else(|| data.get("fqdn"))
            .and_then(|value| value.as_str())
            .unwrap_or(&wl.name)
            .to_string();
        let backends = data
            .get("backends")
            .and_then(|value| value.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|backend| backend.as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let network = data
            .get("network")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        let tls_passthrough = data
            .get("tls_passthrough")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let fqdn = result
            .get("fqdn")
            .or_else(|| result.get("FQDN"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        Ok(Self {
            node_id: 0,
            name,
            backends,
            tls_passthrough,
            network,
            description: wl.description.clone(),
            solution_type: String::new(),
            node_deployment_id: HashMap::new(),
            fqdn,
            name_contract_id: 0,
            contract_id: 0,
        })
    }
}

impl GatewayFQDNProxy {
    pub fn from_workload(wl: &zos::Workload) -> Result<Self, GridError> {
        let data: serde_json::Value = wl.workload_data::<serde_json::Value>()?;
        let fqdn = data
            .get("fqdn")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        let name = data
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or(&wl.name)
            .to_string();
        let backends = data
            .get("backends")
            .and_then(|value| value.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|backend| backend.as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let tls_passthrough = data
            .get("tls_passthrough")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let network = data
            .get("network")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        Ok(Self {
            node_id: 0,
            backends,
            fqdn,
            name,
            tls_passthrough,
            network,
            description: wl.description.clone(),
            solution_type: String::new(),
            contract_id: 0,
            node_deployment_id: HashMap::new(),
        })
    }
}

fn get_flist_checksum(flist: &str) -> Result<String, GridError> {
    if flist.trim().is_empty() {
        return Err(GridError::validation("flist is empty"));
    }
    let checksum_url = format!("{flist}.md5");
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|err| GridError::backend(err.to_string()))?
        .get(&checksum_url)
        .send()
        .map_err(|err| GridError::backend(err.to_string()))?;
    if !response.status().is_success() {
        return Err(GridError::backend(format!(
            "failed to get flist checksum from {checksum_url}: {}",
            response.status()
        )));
    }
    response
        .text()
        .map(|body| body.trim().to_string())
        .map_err(|err| GridError::backend(err.to_string()))
}

pub fn new_network_from_workload(
    workload: &zos::Workload,
    node_id: u32,
) -> Result<ZNet, GridError> {
    if workload.workload_type != zos::NETWORK_TYPE {
        return Err(GridError::validation("workload is not a network workload"));
    }

    let data: NetworkData = serde_json::from_value(workload.data.clone()).map_err(|err| {
        GridError::validation(format!(
            "failed to parse network workload data for {}: {}",
            workload.name, err
        ))
    })?;

    let metadata: NetworkMetaData =
        serde_json::from_str(&workload.metadata).map_err(GridError::from)?;
    let metadata = metadata.normalized();

    let mut add_wg_access = false;
    let mut external_ip = None;
    let mut external_sk = String::new();
    if !metadata.user_accesses.is_empty() {
        if !metadata.user_accesses[0].subnet.is_empty() {
            add_wg_access = true;
            external_ip = Some(metadata.user_accesses[0].subnet.clone());
        }
        external_sk = metadata.user_accesses[0].private_key.clone();
    }

    let mut mycelium_keys = HashMap::new();
    if let Some(mycelium) = data.mycelium.as_ref() {
        if let Some(hex_key) = mycelium.get("hex_key") {
            let key = get_hex_bytes(hex_key);
            if !key.is_empty() {
                mycelium_keys.insert(node_id, key);
            }
        }
    }

    let parsed_network_ip_range = zos::IPNet::parse(&data.ip_range)?;
    let parsed_subnet = zos::IPNet::parse(&data.subnet)?;
    if parsed_subnet.cidr != data.subnet {
        return Err(GridError::validation("invalid subnet"));
    }

    let nodes = vec![node_id];
    let mut nodes_ip_range = HashMap::new();
    nodes_ip_range.insert(node_id, parsed_subnet.cidr.clone());

    let mut wg_port = HashMap::new();
    if data.wg_listen_port > 0 {
        wg_port.insert(node_id, i32::from(data.wg_listen_port));
    }

    let mut keys = HashMap::new();
    if !data.wg_private_key.is_empty() {
        keys.insert(node_id, data.wg_private_key.clone());
    }

    let public_node_id = metadata
        .user_accesses
        .first()
        .map(|access| access.node_id)
        .unwrap_or(0);

    Ok(ZNet {
        name: workload.name.clone(),
        description: workload.description.clone(),
        nodes,
        ip_range: parsed_network_ip_range.cidr,
        add_wg_access,
        mycelium_keys,
        solution_type: String::new(),
        access_wg_config: String::new(),
        external_ip,
        external_sk,
        public_node_id,
        nodes_ip_range,
        node_deployment_id: HashMap::new(),
        wg_port,
        keys,
    })
}

pub fn new_network_light_from_workload(
    workload: &zos::Workload,
    node_id: u32,
) -> Result<ZNetLight, GridError> {
    if workload.workload_type != zos::NETWORK_LIGHT_TYPE {
        return Err(GridError::validation(
            "workload is not a network-light workload",
        ));
    }

    let data: NetworkLightData = serde_json::from_value(workload.data.clone()).map_err(|err| {
        GridError::validation(format!(
            "failed to parse network-light workload data for {}: {}",
            workload.name, err
        ))
    })?;

    let metadata: NetworkMetaData =
        serde_json::from_str(&workload.metadata).map_err(GridError::from)?;
    let metadata = metadata.normalized();

    let mut mycelium_keys = HashMap::new();
    if let Some(mycelium) = data.mycelium.as_ref() {
        if let Some(hex_key) = mycelium.get("hex_key") {
            let key = get_hex_bytes(hex_key);
            if !key.is_empty() {
                mycelium_keys.insert(node_id, key);
            }
        }
    }

    let parsed_subnet = zos::IPNet::parse(&data.subnet)?;
    let mut nodes_ip_range = HashMap::new();
    nodes_ip_range.insert(node_id, parsed_subnet.cidr.clone());

    Ok(ZNetLight {
        name: workload.name.clone(),
        description: workload.description.clone(),
        nodes: vec![node_id],
        solution_type: String::new(),
        ip_range: String::new(),
        nodes_ip_range,
        node_deployment_id: HashMap::new(),
        public_node_id: metadata
            .user_accesses
            .first()
            .map(|access| access.node_id)
            .unwrap_or(0),
        mycelium_keys,
    })
}

pub fn is_master_node(workload: &zos::Workload) -> Result<bool, GridError> {
    let data: serde_json::Value = workload.workload_data()?;
    let env = data
        .get("env")
        .and_then(|value| value.as_object())
        .ok_or_else(|| GridError::validation("workload is not a VM workload"))?;
    Ok(!env.contains_key("K3S_URL") || env.get("K3S_URL").and_then(|v| v.as_str()) == Some(""))
}

pub fn new_k8s_node_from_workload(
    workload: &zos::Workload,
    node_id: u32,
    disk_size: u64,
    computed_ip: &str,
    computed_ip6: &str,
) -> Result<K8sNode, GridError> {
    let data: K8sMachineData = workload.workload_data().map_err(|err| {
        GridError::validation(format!(
            "failed to parse k8s workload {}: {err}",
            workload.name
        ))
    })?;

    let result: VMResult = serde_json::from_value(workload.result.data.clone()).unwrap_or_default();

    let mut ip = String::new();
    let mut network_name = String::new();
    if let Some(interface) = data.network.interfaces.first() {
        ip = interface.get("ip").and_then(extract_ip).unwrap_or_default();
        network_name = interface
            .get("network")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
    }

    let mut cpus = data.compute_cpu;
    if cpus == 0 {
        cpus = data.compute_capacity.cpu;
    }

    let mut memory_mb = data.compute_memory_mb;
    if memory_mb == 0 {
        if data.compute_capacity.memory > 0 {
            memory_mb = data.compute_capacity.memory / zos::MEGABYTE;
        }
    }

    let mycelium_ip_seed = data
        .network
        .mycelium
        .as_ref()
        .and_then(|seed| seed.get("hex_seed"))
        .map(get_hex_bytes)
        .unwrap_or_default();

    let flist_checksum = get_flist_checksum(&data.flist)?;

    Ok(K8sNode {
        vm: VM {
            name: workload.name.clone(),
            node_id,
            network_name,
            description: workload.description.clone(),
            flist: data.flist,
            flist_checksum,
            entrypoint: data.entrypoint,
            public_ip: !computed_ip.is_empty(),
            public_ip6: !computed_ip6.is_empty(),
            planetary: !result.planetary_ip.is_empty()
                || !result.ygg_ip.is_empty()
                || data.network.planetary,
            corex: data.corex,
            ip,
            mycelium_ip_seed,
            cpus,
            memory_mb,
            rootfs_size_mb: data.rootfs_size_mb,
            mounts: data.mounts,
            zlogs: Vec::new(),
            env_vars: data.env,
            computed_ip: computed_ip.to_string(),
            computed_ip6: computed_ip6.to_string(),
            planetary_ip: normalized_planetary_ip(&result),
            mycelium_ip: result.mycelium_ip,
            console_url: result.console_url,
        },
        disk_size_gb: disk_size,
    })
}

pub fn compute_k8s_deployment_resources(
    deployment: &zos::Deployment,
) -> Result<
    (
        HashMap<String, u64>,
        HashMap<String, String>,
        HashMap<String, String>,
    ),
    GridError,
> {
    let mut workload_disk_size: HashMap<String, u64> = HashMap::new();
    let mut workload_computed_ip: HashMap<String, String> = HashMap::new();
    let mut workload_computed_ip6: HashMap<String, String> = HashMap::new();

    let mut public_ips: HashMap<String, String> = HashMap::new();
    let mut public_ip6s: HashMap<String, String> = HashMap::new();

    for workload in &deployment.workloads {
        match workload.workload_type.as_str() {
            zos::PUBLIC_IP_TYPE => {
                let d = serde_json::from_value::<PublicIPResult>(workload.result.data.clone())
                    .map_err(|err| {
                        GridError::validation(format!("failed to read k8s public ip data: {err}"))
                    })?;
                public_ips.insert(workload.name.clone(), extract_ip(&d.ip).unwrap_or_default());
                public_ip6s.insert(
                    workload.name.clone(),
                    extract_ip(&d.ipv6)
                        .or_else(|| extract_ip(&d.ip6))
                        .unwrap_or_default(),
                );
            }
            zos::ZMOUNT_TYPE => {
                let d = workload
                    .data
                    .get("size_gb")
                    .and_then(|value| value.as_u64());
                let size = if let Some(size_gb) = d {
                    size_gb
                } else {
                    workload
                        .data
                        .get("size")
                        .and_then(|value| value.as_u64())
                        .map(|value| value / zos::GIGABYTE)
                        .unwrap_or(0)
                };
                workload_disk_size.insert(workload.name.clone(), size);
            }
            _ => {}
        }
    }

    for workload in &deployment.workloads {
        if workload.workload_type != zos::ZMACHINE_TYPE {
            continue;
        }
        let public_ip_key = format!("{}ip", workload.name);
        let disk_key = format!("{}disk", workload.name);
        workload_computed_ip.insert(
            workload.name.clone(),
            public_ips.get(&public_ip_key).cloned().unwrap_or_default(),
        );
        workload_computed_ip6.insert(
            workload.name.clone(),
            public_ip6s.get(&public_ip_key).cloned().unwrap_or_default(),
        );
        if !workload_disk_size.contains_key(workload.name.as_str()) {
            workload_disk_size.insert(
                workload.name.clone(),
                *workload_disk_size.get(&disk_key).unwrap_or(&0),
            );
        }
    }

    Ok((
        workload_disk_size,
        workload_computed_ip,
        workload_computed_ip6,
    ))
}

impl Disk {
    pub fn validate(&self) -> Result<(), GridError> {
        validate_name(&self.name)?;
        if self.size_gb == 0 {
            return Err(GridError::validation("disk size should be >0"));
        }
        Ok(())
    }
    pub fn zos_workload(&self) -> zos::Workload {
        zos::Workload {
            version: 0,
            name: self.name.clone(),
            workload_type: zos::ZMOUNT_TYPE.to_string(),
            data: serde_json::json!({ "size_gb": self.size_gb }),
            metadata: String::new(),
            description: self.description.clone(),
            result: zos::ResultData::default(),
        }
    }
}

impl Volume {
    pub fn validate(&self) -> Result<(), GridError> {
        validate_name(&self.name)?;
        if self.size_gb == 0 {
            return Err(GridError::validation("volume size should be >0"));
        }
        Ok(())
    }
    pub fn zos_workload(&self) -> zos::Workload {
        zos::Workload {
            version: 0,
            name: self.name.clone(),
            workload_type: zos::VOLUME_TYPE.to_string(),
            data: serde_json::json!({ "size_gb": self.size_gb }),
            metadata: String::new(),
            description: self.description.clone(),
            result: zos::ResultData::default(),
        }
    }
}

impl ZDB {
    pub fn validate(&self) -> Result<(), GridError> {
        validate_name(&self.name)?;
        if self.size_gb == 0 {
            return Err(GridError::validation("zdb size should be >0"));
        }
        if self.mode != "user" && self.mode != "seq" {
            return Err(GridError::validation("unsupported zdb mode"));
        }
        Ok(())
    }

    pub fn zos_workload(&self) -> zos::Workload {
        zos::Workload {
            version: 0,
            name: self.name.clone(),
            workload_type: zos::ZDB_TYPE.to_string(),
            data: serde_json::json!({
                "size_gb": self.size_gb,
                "mode": self.mode,
                "password": self.password,
                "public": self.public
            }),
            metadata: String::new(),
            description: self.description.clone(),
            result: zos::ResultData::default(),
        }
    }
}

impl QSFS {
    pub fn validate(&self) -> Result<(), GridError> {
        validate_name(&self.name)?;
        if self.minimal_shards > self.expected_shards {
            return Err(GridError::validation("minimal_shards > expected_shards"));
        }
        Ok(())
    }

    pub fn zos_workload(&self) -> Result<zos::Workload, GridError> {
        Ok(zos::Workload {
            version: 0,
            name: self.name.clone(),
            workload_type: zos::QUANTUM_SAFE_FS_TYPE.to_string(),
            data: serde_json::to_value(self)?,
            metadata: String::new(),
            description: self.description.clone(),
            result: zos::ResultData::default(),
        })
    }
}

pub fn construct_public_ip_workload(name: &str, ipv4: bool, ipv6: bool) -> zos::Workload {
    zos::Workload {
        version: 0,
        name: name.to_string(),
        workload_type: zos::PUBLIC_IP_TYPE.to_string(),
        data: serde_json::json!({ "v4": ipv4, "v6": ipv6 }),
        metadata: String::new(),
        description: String::new(),
        result: zos::ResultData::default(),
    }
}

pub fn new_disk_from_workload(workload: &zos::Workload) -> Result<Disk, GridError> {
    let data: serde_json::Value = workload.data.clone();
    Ok(Disk {
        name: workload.name.clone(),
        size_gb: data
            .get("size_gb")
            .and_then(|v| v.as_u64())
            .unwrap_or_default(),
        description: workload.description.clone(),
    })
}

pub fn new_volume_from_workload(workload: &zos::Workload) -> Result<Volume, GridError> {
    let data: serde_json::Value = workload.data.clone();
    Ok(Volume {
        name: workload.name.clone(),
        size_gb: data
            .get("size_gb")
            .and_then(|v| v.as_u64())
            .unwrap_or_default(),
        description: workload.description.clone(),
    })
}

pub fn new_vm_from_workload(
    workload: &zos::Workload,
    deployment: &zos::Deployment,
    node_id: u32,
) -> Result<VM, GridError> {
    let data: VMWorkloadData =
        serde_json::from_value(workload.data.clone()).map_err(GridError::from)?;
    let result: VMResult = serde_json::from_value(workload.result.data.clone()).unwrap_or_default();
    let flist_checksum = get_flist_checksum(&data.flist)?;
    let (computed_ip, computed_ip6, public_ip, public_ip6) =
        resolve_public_ips(deployment, &workload.name);

    Ok(VM {
        name: workload.name.clone(),
        node_id,
        network_name: data.network_name,
        description: workload.description.clone(),
        flist: data.flist,
        flist_checksum,
        entrypoint: data.entrypoint,
        public_ip,
        public_ip6,
        planetary: !result.planetary_ip.is_empty() || !result.ygg_ip.is_empty(),
        corex: data.corex,
        ip: data.ip,
        mycelium_ip_seed: Vec::new(),
        cpus: data.compute_cpu,
        memory_mb: data.compute_memory_mb,
        rootfs_size_mb: data.rootfs_size_mb,
        mounts: data.mounts,
        zlogs: Vec::new(),
        env_vars: data.env_vars,
        computed_ip,
        computed_ip6,
        planetary_ip: normalized_planetary_ip(&result),
        mycelium_ip: result.mycelium_ip,
        console_url: result.console_url,
    })
}

pub fn new_vm_light_from_workload(
    workload: &zos::Workload,
    _deployment: &zos::Deployment,
    node_id: u32,
) -> Result<VMLight, GridError> {
    let data: VMLightWorkloadData =
        serde_json::from_value(workload.data.clone()).map_err(GridError::from)?;
    let result: VMLightResult =
        serde_json::from_value(workload.result.data.clone()).unwrap_or_default();
    let flist_checksum = get_flist_checksum(&data.flist)?;
    Ok(VMLight {
        name: workload.name.clone(),
        node_id,
        network_name: data.network_name,
        description: workload.description.clone(),
        flist: data.flist,
        flist_checksum,
        entrypoint: data.entrypoint,
        corex: data.corex,
        ip: data.ip,
        mycelium_ip_seed: Vec::new(),
        cpus: data.compute_cpu,
        memory_mb: data.compute_memory_mb,
        rootfs_size_mb: data.rootfs_size_mb,
        mounts: data.mounts,
        zlogs: Vec::new(),
        env_vars: data.env_vars,
        mycelium_ip: result.mycelium_ip,
        console_url: result.console_url,
        computed_ip: String::new(),
        computed_ip6: String::new(),
    })
}

pub fn new_zdb_from_workload(workload: &zos::Workload) -> Result<ZDB, GridError> {
    let size = workload
        .data
        .get("size_gb")
        .and_then(|v| v.as_u64())
        .unwrap_or_default();
    let mode = workload
        .data
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("user")
        .to_string();
    let public = workload
        .data
        .get("public")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok(ZDB {
        name: workload.name.clone(),
        password: String::new(),
        public,
        size_gb: size,
        description: workload.description.clone(),
        mode,
        ips: Vec::new(),
        port: 0,
        namespace: String::new(),
    })
}

pub fn new_qsfs_from_workload(workload: &zos::Workload) -> Result<QSFS, GridError> {
    serde_json::from_value(workload.data.clone()).map_err(GridError::from)
}

pub fn new_deployment_from_zos_deployment(
    deployment: zos::Deployment,
    node_id: u32,
) -> Result<Deployment, GridError> {
    let metadata = parse_deployment_data(&deployment.metadata)?;
    let mut network_name = String::new();
    let mut output = Deployment::new(
        &metadata.name,
        node_id,
        &metadata.project_name,
        None,
        "",
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    output.contract_id = deployment.contract_id;
    output
        .node_deployment_id
        .insert(node_id, deployment.contract_id);

    for wl in &deployment.workloads {
        match wl.workload_type.as_str() {
            zos::ZMOUNT_TYPE => {
                let mut disk = new_disk_from_workload(wl)?;
                disk.name = wl.name.clone();
                output.disks.push(disk);
            }
            zos::VOLUME_TYPE => {
                let mut volume = new_volume_from_workload(wl)?;
                volume.name = wl.name.clone();
                output.volumes.push(volume);
            }
            zos::ZDB_TYPE => {
                let mut zdb = new_zdb_from_workload(wl)?;
                zdb.name = wl.name.clone();
                output.zdbs.push(zdb);
            }
            zos::ZMACHINE_TYPE => {
                let mut vm = new_vm_from_workload(wl, &deployment, node_id)?;
                vm.node_id = node_id;
                vm.name = wl.name.clone();
                network_name = vm.network_name.clone();
                output.vms.push(vm);
            }
            zos::ZMACHINE_LIGHT_TYPE => {
                let mut vm = new_vm_light_from_workload(wl, &deployment, node_id)?;
                vm.node_id = node_id;
                vm.name = wl.name.clone();
                network_name = vm.network_name.clone();
                output.vms_light.push(vm);
            }
            zos::QUANTUM_SAFE_FS_TYPE => {
                let mut q = new_qsfs_from_workload(wl)?;
                q.name = wl.name.clone();
                output.qsfs.push(q);
            }
            _ => {}
        }
    }

    output.network_name = network_name;
    Ok(output)
}

fn resolve_public_ips(
    deployment: &zos::Deployment,
    workload_name: &str,
) -> (String, String, bool, bool) {
    let public_ip_workload_name = format!("{workload_name}ip");
    let Some(wl) = deployment
        .workloads
        .iter()
        .find(|candidate| candidate.name == public_ip_workload_name)
    else {
        return (String::new(), String::new(), false, false);
    };

    if !wl.result.is_okay() {
        return (String::new(), String::new(), false, false);
    }

    let ip_data: PublicIPResult =
        serde_json::from_value(wl.result.data.clone()).unwrap_or_default();
    let computed_ip = extract_ip(&ip_data.ip);
    let computed_ip6 = extract_ip(&ip_data.ipv6).or_else(|| extract_ip(&ip_data.ip6));
    (
        computed_ip.clone().unwrap_or_default(),
        computed_ip6.clone().unwrap_or_default(),
        computed_ip.is_some(),
        computed_ip6.is_some(),
    )
}

fn extract_ip(raw: &serde_json::Value) -> Option<String> {
    match raw {
        serde_json::Value::String(value) => normalize_ip(value),
        serde_json::Value::Object(map) => map
            .get("ip")
            .or_else(|| map.get("addr"))
            .or_else(|| map.get("ip_address"))
            .or_else(|| map.get("address"))
            .and_then(|v| v.as_str())
            .and_then(normalize_ip),
        _ => None,
    }
}

fn normalize_ip(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(addr) = value.parse::<IpAddr>() {
        return Some(addr.to_string());
    }
    value
        .split('/')
        .next()
        .and_then(|ip| ip.parse::<IpAddr>().ok())
        .map(|addr| addr.to_string())
}

fn normalized_planetary_ip(result: &VMResult) -> String {
    if !result.planetary_ip.is_empty() {
        return result.planetary_ip.clone();
    }
    result.ygg_ip.clone()
}

fn validate_name(name: &str) -> Result<(), GridError> {
    if name.is_empty() {
        return Err(GridError::validation("name cannot be empty"));
    }
    if name.len() > 36 {
        return Err(GridError::validation("name cannot exceed 36 chars"));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(GridError::validation("name has invalid characters"));
    }
    Ok(())
}

pub fn parse_ip(ip: &str) -> Option<IpAddr> {
    IpAddr::from_str(ip).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zos;
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn spawn_http_server(routes: HashMap<String, String>, expected_requests: usize) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let addr = listener.local_addr().expect("local addr");
        thread::spawn(move || {
            for _ in 0..expected_requests {
                let (mut stream, _) = listener.accept().expect("accept request");
                let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
                let mut request_line = String::new();
                reader
                    .read_line(&mut request_line)
                    .expect("read request line");
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).expect("read header");
                    if header == "\r\n" || header.is_empty() {
                        break;
                    }
                }
                let path = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_string();
                let body = routes.get(&path).cloned().unwrap_or_default();
                let status = if routes.contains_key(&path) {
                    "HTTP/1.1 200 OK"
                } else {
                    "HTTP/1.1 404 Not Found"
                };
                let response = format!(
                    "{status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write response");
                let mut drain = Vec::new();
                let _ = reader.read_to_end(&mut drain);
            }
        });
        format!("http://{}", addr)
    }

    #[test]
    fn vm_from_workload_reads_public_and_runtime_result() {
        let base_url = spawn_http_server(
            HashMap::from([("/vm1.flist.md5".to_string(), "checksum-vm1\n".to_string())]),
            1,
        );
        let vm_wl = zos::Workload {
            version: 0,
            name: "vm1".to_string(),
            workload_type: zos::ZMACHINE_TYPE.to_string(),
            data: serde_json::json!({
                "network_name": "net1",
                "ip": "192.168.1.10",
                "compute_cpu": 2u8,
                "compute_memory_mb": 4096u64,
                "rootfs_size_mb": 20u64,
                "flist": format!("{base_url}/vm1.flist"),
                "entrypoint": "/bin/sh",
            }),
            metadata: String::new(),
            description: "vm".to_string(),
            result: zos::ResultData {
                created: 0,
                state: zos::STATE_OK.to_string(),
                error: String::new(),
                data: serde_json::json!({
                    "planetary_ip": "2001:db8::1",
                    "mycelium_ip": "mycelium-vm1",
                    "console_url": "https://console.example",
                }),
            },
        };

        let public_ip = zos::Workload {
            version: 0,
            name: "vm1ip".to_string(),
            workload_type: zos::PUBLIC_IP_TYPE.to_string(),
            data: serde_json::json!({ "v4": true, "v6": true }),
            metadata: String::new(),
            description: String::new(),
            result: zos::ResultData {
                created: 0,
                state: zos::STATE_OK.to_string(),
                error: String::new(),
                data: serde_json::json!({
                    "ip": "203.0.113.5/32",
                    "ipv6": "2001:db8::2/128",
                }),
            },
        };

        let deployment = zos::Deployment {
            version: 0,
            twin_id: 1,
            contract_id: 10,
            metadata: "".to_string(),
            description: String::new(),
            expiration: 0,
            signature_requirement: zos::SignatureRequirement::default(),
            workloads: vec![vm_wl.clone(), public_ip],
        };

        let vm = new_vm_from_workload(&vm_wl, &deployment, 1).expect("vm");

        assert_eq!(vm.flist_checksum, "checksum-vm1");
        assert_eq!(vm.computed_ip, "203.0.113.5");
        assert_eq!(vm.computed_ip6, "2001:db8::2");
        assert!(vm.public_ip);
        assert!(vm.public_ip6);
        assert_eq!(vm.planetary_ip, "2001:db8::1");
        assert_eq!(vm.mycelium_ip, "mycelium-vm1");
        assert_eq!(vm.console_url, "https://console.example");
    }

    #[test]
    fn vm_light_from_workload_reads_runtime_result() {
        let base_url = spawn_http_server(
            HashMap::from([("/vm2.flist.md5".to_string(), "checksum-vm2".to_string())]),
            1,
        );
        let vm_wl = zos::Workload {
            version: 0,
            name: "vm2".to_string(),
            workload_type: zos::ZMACHINE_LIGHT_TYPE.to_string(),
            data: serde_json::json!({
                "network_name": "net1",
                "ip": "192.168.1.20",
                "compute_cpu": 1u8,
                "compute_memory_mb": 2048u64,
                "rootfs_size_mb": 20u64,
                "flist": format!("{base_url}/vm2.flist"),
                "entrypoint": "/bin/bash",
            }),
            metadata: String::new(),
            description: "vmlight".to_string(),
            result: zos::ResultData {
                created: 0,
                state: zos::STATE_OK.to_string(),
                error: String::new(),
                data: serde_json::json!({
                    "mycelium_ip": "mycelium-vm2",
                    "console_url": "https://console-2.example",
                }),
            },
        };
        let deployment = zos::Deployment {
            version: 0,
            twin_id: 1,
            contract_id: 11,
            metadata: String::new(),
            description: String::new(),
            expiration: 0,
            signature_requirement: zos::SignatureRequirement::default(),
            workloads: vec![vm_wl.clone()],
        };

        let vm = new_vm_light_from_workload(&vm_wl, &deployment, 1).expect("vm-light");
        assert_eq!(vm.flist_checksum, "checksum-vm2");
        assert_eq!(vm.mycelium_ip, "mycelium-vm2");
        assert_eq!(vm.console_url, "https://console-2.example");
    }
}
