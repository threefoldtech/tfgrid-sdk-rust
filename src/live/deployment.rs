use std::collections::HashMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::json;
use subxt_signer::sr25519::Keypair;

use crate::{GridError, workloads, zos};

use super::{
    FullNetworkTarget, NetworkTarget, VmDeployment, VmLightDeployment, VmLightSpec, VmSpec,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DeployWorkload {
    pub version: u32,
    pub name: String,
    #[serde(rename = "type")]
    pub workload_type: String,
    pub data: serde_json::Value,
    pub metadata: String,
    pub description: String,
    pub result: zos::ResultData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DeployDeployment {
    pub version: u32,
    pub twin_id: u32,
    pub contract_id: u64,
    pub metadata: String,
    pub description: String,
    pub expiration: i64,
    signature_requirement: SignatureRequirement,
    pub workloads: Vec<DeployWorkload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignatureRequest {
    twin_id: u32,
    required: bool,
    weight: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Signature {
    twin_id: u32,
    signature: String,
    signature_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignatureRequirement {
    requests: Vec<SignatureRequest>,
    weight_required: u32,
    signatures: Vec<Signature>,
    signature_style: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NetworkLightData {
    subnet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mycelium: Option<MyceliumData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MyceliumData {
    #[serde(rename = "hex_key", with = "super::hex_bytes")]
    key: Vec<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    peers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MyceliumIpData {
    network: String,
    #[serde(rename = "hex_seed", with = "super::hex_bytes")]
    seed: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MachineInterfaceData {
    network: String,
    ip: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MachineNetworkLightData {
    #[serde(skip_serializing_if = "Option::is_none")]
    mycelium: Option<MyceliumIpData>,
    interfaces: Vec<MachineInterfaceData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MachineCapacityData {
    cpu: u8,
    memory: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MachineMountData {
    name: String,
    mountpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ZMachineLightData {
    flist: String,
    network: MachineNetworkLightData,
    size: u64,
    compute_capacity: MachineCapacityData,
    #[serde(default)]
    mounts: Vec<MachineMountData>,
    entrypoint: String,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    corex: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    gpu: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FullNetworkData {
    #[serde(rename = "ip_range")]
    ip_range: String,
    subnet: String,
    #[serde(rename = "wireguard_private_key")]
    wireguard_private_key: String,
    #[serde(rename = "wireguard_listen_port")]
    wireguard_listen_port: u16,
    peers: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mycelium: Option<MyceliumData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MachineNetworkData {
    #[serde(rename = "public_ip")]
    public_ip: String,
    planetary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    mycelium: Option<MyceliumIpData>,
    interfaces: Vec<MachineInterfaceData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ZMachineData {
    flist: String,
    network: MachineNetworkData,
    size: u64,
    compute_capacity: MachineCapacityData,
    #[serde(default)]
    mounts: Vec<MachineMountData>,
    entrypoint: String,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    corex: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    gpu: Vec<String>,
}

pub(crate) fn validate_vm_light_request(request: &VmLightDeployment) -> Result<(), GridError> {
    if request.vm.cpu == 0 {
        return Err(GridError::validation("vm cpu must be greater than zero"));
    }
    if request.vm.memory_bytes == 0 {
        return Err(GridError::validation(
            "vm memory_bytes must be greater than zero",
        ));
    }
    if request.vm.rootfs_size_bytes == 0 {
        return Err(GridError::validation(
            "vm rootfs_size_bytes must be greater than zero",
        ));
    }
    if request.vm.flist.trim().is_empty() {
        return Err(GridError::validation("vm flist must not be empty"));
    }
    if request.vm.entrypoint.trim().is_empty() {
        return Err(GridError::validation("vm entrypoint must not be empty"));
    }
    for volume in &request.vm.volumes {
        if volume.name.trim().is_empty() {
            return Err(GridError::validation("volume name must not be empty"));
        }
        if volume.size_bytes == 0 {
            return Err(GridError::validation(
                "volume size_bytes must be greater than zero",
            ));
        }
        if volume.mountpoint.trim().is_empty() {
            return Err(GridError::validation("volume mountpoint must not be empty"));
        }
    }
    match &request.network {
        NetworkTarget::Create(network) => {
            if let Some(name) = &network.name
                && name.trim().is_empty()
            {
                return Err(GridError::validation("network name must not be empty"));
            }
            if let Some(subnet) = &network.subnet {
                super::vm_ip_from_subnet(subnet)?;
            }
        }
        NetworkTarget::Existing(existing) => {
            if existing.name.trim().is_empty() {
                return Err(GridError::validation(
                    "existing network name must not be empty",
                ));
            }
            if existing.ip.trim().is_empty() {
                return Err(GridError::validation(
                    "existing network ip must not be empty",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_vm_request(request: &VmDeployment) -> Result<(), GridError> {
    if request.vm.cpu == 0 {
        return Err(GridError::validation("vm cpu must be greater than zero"));
    }
    if request.vm.memory_bytes == 0 {
        return Err(GridError::validation(
            "vm memory_bytes must be greater than zero",
        ));
    }
    if request.vm.rootfs_size_bytes == 0 {
        return Err(GridError::validation(
            "vm rootfs_size_bytes must be greater than zero",
        ));
    }
    if request.vm.flist.trim().is_empty() {
        return Err(GridError::validation("vm flist must not be empty"));
    }
    for volume in &request.vm.volumes {
        if volume.name.trim().is_empty() {
            return Err(GridError::validation("volume name must not be empty"));
        }
        if volume.size_bytes == 0 {
            return Err(GridError::validation(
                "volume size_bytes must be greater than zero",
            ));
        }
        if volume.mountpoint.trim().is_empty() {
            return Err(GridError::validation("volume mountpoint must not be empty"));
        }
    }
    match &request.network {
        FullNetworkTarget::Create(network) => {
            if let Some(ip_range) = &network.ip_range
                && !ip_range.ends_with("/16")
            {
                return Err(GridError::validation("network ip_range must be a /16"));
            }
            if let Some(subnet) = &network.subnet {
                super::vm_ip_from_subnet(subnet)?;
            }
        }
        FullNetworkTarget::Existing(existing) => {
            if existing.name.trim().is_empty() {
                return Err(GridError::validation(
                    "existing network name must not be empty",
                ));
            }
            if existing.ip.trim().is_empty() {
                return Err(GridError::validation(
                    "existing network ip must not be empty",
                ));
            }
        }
    }
    Ok(())
}

impl DeployDeployment {
    pub(crate) fn new(twin_id: u32, metadata: String, workloads: Vec<DeployWorkload>) -> Self {
        Self {
            version: 0,
            twin_id,
            contract_id: 0,
            metadata,
            description: String::new(),
            expiration: 0,
            signature_requirement: SignatureRequirement {
                requests: vec![SignatureRequest {
                    twin_id,
                    required: false,
                    weight: 1,
                }],
                weight_required: 1,
                signatures: Vec::new(),
                signature_style: String::new(),
            },
            workloads,
        }
    }
}

pub(crate) fn build_network_light(
    name: &str,
    subnet: &str,
    mycelium_key: Vec<u8>,
) -> DeployWorkload {
    DeployWorkload {
        version: 0,
        name: name.to_string(),
        workload_type: zos::NETWORK_LIGHT_TYPE.to_string(),
        data: serde_json::to_value(NetworkLightData {
            subnet: subnet.to_string(),
            mycelium: Some(MyceliumData {
                key: mycelium_key,
                peers: Vec::new(),
            }),
        })
        .unwrap_or_default(),
        metadata: serde_json::to_string(&super::NetworkWorkloadMetadata {
            version: workloads::VERSION4,
            user_accesses: None,
        })
        .unwrap_or_default(),
        description: "network to deploy vm with mycelium".to_string(),
        result: empty_result_data(),
    }
}

pub(crate) fn build_network(
    name: &str,
    ip_range: &str,
    subnet: &str,
    wireguard_private_key: String,
    wireguard_listen_port: u16,
    mycelium_key: Option<Vec<u8>>,
) -> DeployWorkload {
    DeployWorkload {
        version: 0,
        name: name.to_string(),
        workload_type: zos::NETWORK_TYPE.to_string(),
        data: serde_json::to_value(FullNetworkData {
            ip_range: ip_range.to_string(),
            subnet: subnet.to_string(),
            wireguard_private_key,
            wireguard_listen_port,
            peers: Vec::new(),
            mycelium: mycelium_key.map(|key| MyceliumData {
                key,
                peers: Vec::new(),
            }),
        })
        .unwrap_or_default(),
        metadata: serde_json::to_string(&super::NetworkWorkloadMetadata {
            version: workloads::VERSION4,
            user_accesses: None,
        })
        .unwrap_or_default(),
        description: "network to deploy vm".to_string(),
        result: empty_result_data(),
    }
}

pub(crate) fn build_vm_light(
    name: &str,
    network_name: &str,
    ip: &str,
    spec: &VmLightSpec,
) -> Vec<DeployWorkload> {
    let mut workloads = Vec::new();
    for volume in &spec.volumes {
        workloads.push(build_volume_workload(
            &volume.name,
            volume.size_bytes,
            &volume.description,
        ));
    }
    let data = ZMachineLightData {
        flist: spec.flist.clone(),
        network: MachineNetworkLightData {
            mycelium: Some(MyceliumIpData {
                network: network_name.to_string(),
                seed: spec
                    .mycelium_seed
                    .clone()
                    .unwrap_or_else(|| super::random_bytes(zos::MYCELIUM_IP_SEED_LEN)),
            }),
            interfaces: vec![MachineInterfaceData {
                network: network_name.to_string(),
                ip: ip.to_string(),
            }],
        },
        size: spec.rootfs_size_bytes,
        compute_capacity: MachineCapacityData {
            cpu: spec.cpu,
            memory: spec.memory_bytes,
        },
        mounts: spec
            .volumes
            .iter()
            .map(|mount| MachineMountData {
                name: mount.name.clone(),
                mountpoint: mount.mountpoint.clone(),
            })
            .chain(spec.mounts.iter().map(|mount| MachineMountData {
                name: mount.name.clone(),
                mountpoint: mount.mountpoint.clone(),
            }))
            .collect(),
        entrypoint: spec.entrypoint.clone(),
        env: spec.env.clone(),
        corex: spec.corex,
        gpu: spec.gpu.clone(),
    };
    workloads.push(DeployWorkload {
        version: 0,
        name: name.to_string(),
        workload_type: zos::ZMACHINE_LIGHT_TYPE.to_string(),
        data: serde_json::to_value(data).unwrap_or_default(),
        metadata: String::new(),
        description: String::new(),
        result: empty_result_data(),
    });
    workloads
}

pub(crate) fn build_vm(
    name: &str,
    network_name: &str,
    ip: &str,
    spec: &VmSpec,
) -> Vec<DeployWorkload> {
    let public_ip_name = if spec.public_ipv4 || spec.public_ipv6 {
        format!("{name}ip")
    } else {
        String::new()
    };
    let mut workloads = Vec::new();
    if !public_ip_name.is_empty() {
        workloads.push(from_zos_workload(workloads::construct_public_ip_workload(
            &public_ip_name,
            spec.public_ipv4,
            spec.public_ipv6,
        )));
    }
    for volume in &spec.volumes {
        workloads.push(build_volume_workload(
            &volume.name,
            volume.size_bytes,
            &volume.description,
        ));
    }
    let data = ZMachineData {
        flist: spec.flist.clone(),
        network: MachineNetworkData {
            public_ip: public_ip_name,
            planetary: spec.planetary,
            mycelium: spec.mycelium_seed.clone().map(|seed| MyceliumIpData {
                network: network_name.to_string(),
                seed,
            }),
            interfaces: vec![MachineInterfaceData {
                network: network_name.to_string(),
                ip: ip.to_string(),
            }],
        },
        size: spec.rootfs_size_bytes,
        compute_capacity: MachineCapacityData {
            cpu: spec.cpu,
            memory: spec.memory_bytes,
        },
        mounts: spec
            .volumes
            .iter()
            .map(|mount| MachineMountData {
                name: mount.name.clone(),
                mountpoint: mount.mountpoint.clone(),
            })
            .collect(),
        entrypoint: spec.entrypoint.clone(),
        env: spec.env.clone(),
        corex: spec.corex,
        gpu: spec.gpu.clone(),
    };
    workloads.push(DeployWorkload {
        version: 0,
        name: name.to_string(),
        workload_type: zos::ZMACHINE_TYPE.to_string(),
        data: serde_json::to_value(data).unwrap_or_default(),
        metadata: String::new(),
        description: String::new(),
        result: empty_result_data(),
    });
    workloads
}

fn from_zos_workload(workload: zos::Workload) -> DeployWorkload {
    DeployWorkload {
        version: workload.version,
        name: workload.name,
        workload_type: workload.workload_type,
        data: workload.data,
        metadata: workload.metadata,
        description: workload.description,
        result: workload.result,
    }
}

fn build_volume_workload(name: &str, size_bytes: u64, description: &str) -> DeployWorkload {
    DeployWorkload {
        version: 0,
        name: name.to_string(),
        workload_type: zos::VOLUME_TYPE.to_string(),
        data: json!({ "size": size_bytes }),
        metadata: String::new(),
        description: description.to_string(),
        result: empty_result_data(),
    }
}

pub(crate) fn sign_deployment(
    deployment: &mut DeployDeployment,
    twin_id: u32,
    signer: &Keypair,
) -> Result<(), GridError> {
    let challenge = deployment_hash_bytes(deployment)?;
    let signature = hex::encode(super::substrate_sign(signer, &challenge).as_ref());
    deployment.signature_requirement.signatures.push(Signature {
        twin_id,
        signature,
        signature_type: "sr25519".to_string(),
    });
    Ok(())
}

pub(crate) fn deployment_hash_hex(deployment: &DeployDeployment) -> Result<String, GridError> {
    Ok(hex::encode(deployment_hash_bytes(deployment)?))
}

fn deployment_hash_bytes(deployment: &DeployDeployment) -> Result<Vec<u8>, GridError> {
    let mut challenge = String::new();
    write!(&mut challenge, "{}", deployment.version)
        .map_err(|err| GridError::backend(err.to_string()))?;
    write!(&mut challenge, "{}", deployment.twin_id)
        .map_err(|err| GridError::backend(err.to_string()))?;
    challenge.push_str(&deployment.metadata);
    challenge.push_str(&deployment.description);
    write!(&mut challenge, "{}", deployment.expiration)
        .map_err(|err| GridError::backend(err.to_string()))?;
    for workload in &deployment.workloads {
        workload_challenge(&mut challenge, workload)?;
    }
    for request in &deployment.signature_requirement.requests {
        write!(
            &mut challenge,
            "{}{}{}",
            request.twin_id, request.required, request.weight
        )
        .map_err(|err| GridError::backend(err.to_string()))?;
    }
    write!(
        &mut challenge,
        "{}{}",
        deployment.signature_requirement.weight_required,
        deployment.signature_requirement.signature_style
    )
    .map_err(|err| GridError::backend(err.to_string()))?;
    Ok(md5::compute(challenge.as_bytes()).0.to_vec())
}

fn workload_challenge(out: &mut String, workload: &DeployWorkload) -> Result<(), GridError> {
    write!(out, "{}", workload.version).map_err(|err| GridError::backend(err.to_string()))?;
    out.push_str(&workload.name);
    out.push_str(&workload.workload_type);
    out.push_str(&workload.metadata);
    out.push_str(&workload.description);
    match workload.workload_type.as_str() {
        zos::NETWORK_LIGHT_TYPE => {
            let data: NetworkLightData =
                serde_json::from_value(workload.data.clone()).map_err(GridError::from)?;
            out.push_str("<nil>");
            out.push_str(&data.subnet);
            out.push_str("");
            out.push('0');
            if let Some(mycelium) = data.mycelium {
                out.push_str(&hex::encode(mycelium.key));
                for peer in mycelium.peers {
                    out.push_str(&peer);
                }
            }
        }
        zos::ZMACHINE_LIGHT_TYPE => {
            let data: ZMachineLightData =
                serde_json::from_value(workload.data.clone()).map_err(GridError::from)?;
            out.push_str(&data.flist);
            for interface in data.network.interfaces {
                out.push_str(&interface.network);
                out.push_str(&interface.ip);
            }
            if let Some(mycelium) = data.network.mycelium {
                out.push_str(&mycelium.network);
                out.push_str(&hex::encode(mycelium.seed));
            }
            write!(out, "{}", data.size).map_err(|err| GridError::backend(err.to_string()))?;
            write!(
                out,
                "{}{}",
                data.compute_capacity.cpu, data.compute_capacity.memory
            )
            .map_err(|err| GridError::backend(err.to_string()))?;
            for mount in data.mounts {
                out.push_str(&mount.name);
                out.push_str(&mount.mountpoint);
            }
            out.push_str(&data.entrypoint);
            let mut keys: Vec<_> = data.env.keys().cloned().collect();
            keys.sort();
            for key in keys {
                out.push_str(&key);
                out.push('=');
                out.push_str(&data.env[&key]);
            }
            for gpu in data.gpu {
                out.push_str(&gpu);
            }
        }
        zos::VOLUME_TYPE => {
            let size = workload
                .data
                .get("size")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| GridError::validation("live volume workload missing size"))?;
            write!(out, "{size}").map_err(|err| GridError::backend(err.to_string()))?;
        }
        zos::PUBLIC_IP_TYPE => {
            let v4 = workload
                .data
                .get("v4")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let v6 = workload
                .data
                .get("v6")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            write!(out, "{v4}{v6}").map_err(|err| GridError::backend(err.to_string()))?;
        }
        zos::NETWORK_TYPE => {
            let data: FullNetworkData =
                serde_json::from_value(workload.data.clone()).map_err(GridError::from)?;
            out.push_str(&data.ip_range);
            out.push_str(&data.subnet);
            out.push_str(&data.wireguard_private_key);
            write!(out, "{}", data.wireguard_listen_port)
                .map_err(|err| GridError::backend(err.to_string()))?;
            for peer in data.peers {
                out.push_str(&peer.to_string());
            }
            if let Some(mycelium) = data.mycelium {
                out.push_str(&hex::encode(mycelium.key));
                for peer in mycelium.peers {
                    out.push_str(&peer);
                }
            }
        }
        zos::ZMACHINE_TYPE => {
            let data: ZMachineData =
                serde_json::from_value(workload.data.clone()).map_err(GridError::from)?;
            out.push_str(&data.flist);
            out.push_str(&data.network.public_ip);
            write!(out, "{}", data.network.planetary)
                .map_err(|err| GridError::backend(err.to_string()))?;
            for interface in data.network.interfaces {
                out.push_str(&interface.network);
                out.push_str(&interface.ip);
            }
            if let Some(mycelium) = data.network.mycelium {
                out.push_str(&mycelium.network);
                out.push_str(&hex::encode(mycelium.seed));
            }
            write!(out, "{}", data.size).map_err(|err| GridError::backend(err.to_string()))?;
            write!(
                out,
                "{}{}",
                data.compute_capacity.cpu, data.compute_capacity.memory
            )
            .map_err(|err| GridError::backend(err.to_string()))?;
            for mount in data.mounts {
                out.push_str(&mount.name);
                out.push_str(&mount.mountpoint);
            }
            out.push_str(&data.entrypoint);
            let mut keys: Vec<_> = data.env.keys().cloned().collect();
            keys.sort();
            for key in keys {
                out.push_str(&key);
                out.push('=');
                out.push_str(&data.env[&key]);
            }
            for gpu in data.gpu {
                out.push_str(&gpu);
            }
        }
        other => {
            return Err(GridError::validation(format!(
                "unsupported live workload type {other}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn public_ip_count(workloads: &[DeployWorkload]) -> u32 {
    workloads
        .iter()
        .filter(|workload| workload.workload_type == zos::PUBLIC_IP_TYPE)
        .count() as u32
}

pub(crate) fn empty_result_data() -> zos::ResultData {
    zos::ResultData {
        created: 0,
        state: String::new(),
        error: String::new(),
        data: serde_json::Value::Null,
    }
}
