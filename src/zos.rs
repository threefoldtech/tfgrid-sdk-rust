//! Lightweight Rust facsimile of `tfgrid-sdk-rust/zos` types.

use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

use crate::error::GridError;

pub const MYCELIUM_KEY_LEN: usize = 32;
pub const MYCELIUM_IP_SEED_LEN: usize = 6;

pub const KILOBYTE: u64 = 1024;
pub const MEGABYTE: u64 = KILOBYTE * 1024;
pub const GIGABYTE: u64 = MEGABYTE * 1024;
pub const TERABYTE: u64 = GIGABYTE * 1024;

pub const ZMOUNT_TYPE: &str = "zmount";
pub const NETWORK_TYPE: &str = "network";
pub const NETWORK_LIGHT_TYPE: &str = "network-light";
pub const ZDB_TYPE: &str = "zdb";
pub const ZMACHINE_TYPE: &str = "zmachine";
pub const ZMACHINE_LIGHT_TYPE: &str = "zmachine-light";
pub const VOLUME_TYPE: &str = "volume";
pub const PUBLIC_IP_TYPE: &str = "ip";
pub const GATEWAY_NAME_PROXY_TYPE: &str = "gateway-name-proxy";
pub const GATEWAY_FQDN_PROXY_TYPE: &str = "gateway-fqdn-proxy";
pub const QUANTUM_SAFE_FS_TYPE: &str = "qsfs";
pub const ZLOGS_TYPE: &str = "zlogs";

pub const STATE_INIT: &str = "init";
pub const STATE_UNCHANGED: &str = "unchanged";
pub const STATE_ERROR: &str = "error";
pub const STATE_OK: &str = "ok";
pub const STATE_DELETED: &str = "deleted";
pub const STATE_PAUSED: &str = "paused";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IPNet {
    pub cidr: String,
}

impl IPNet {
    pub fn parse(txt: &str) -> Result<Self, GridError> {
        txt.parse::<IpNet>()
            .map_err(|e| GridError::Validation(format!("invalid cidr: {e}")))?;
        Ok(Self {
            cidr: txt.to_string(),
        })
    }

    pub fn new(addr: &str) -> Self {
        Self {
            cidr: addr.to_string(),
        }
    }

    pub fn contains(&self, ip: &str) -> bool {
        let Ok(net) = self.cidr.parse::<IpNet>() else {
            return false;
        };
        let Ok(ip_addr): Result<IpAddr, _> = ip.parse() else {
            return false;
        };
        net.contains::<&IpAddr>(&ip_addr)
    }

    pub fn string(&self) -> &str {
        &self.cidr
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultData {
    pub created: i64,
    pub state: String,
    pub error: String,
    pub data: serde_json::Value,
}

impl Default for ResultData {
    fn default() -> Self {
        Self {
            created: 0,
            state: STATE_INIT.to_string(),
            error: String::new(),
            data: serde_json::json!(null),
        }
    }
}

impl ResultData {
    pub fn is_okay(&self) -> bool {
        self.state == STATE_OK || self.state == STATE_PAUSED
    }

    pub fn is_any(&self, states: &[&str]) -> bool {
        states.iter().any(|state| state == &&self.state)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Workload {
    pub version: u32,
    pub name: String,
    #[serde(rename = "type")]
    pub workload_type: String,
    pub data: serde_json::Value,
    pub metadata: String,
    pub description: String,
    #[serde(default)]
    pub result: ResultData,
}

impl Workload {
    pub fn workload_data<T: serde::de::DeserializeOwned>(&self) -> Result<T, GridError> {
        serde_json::from_value(self.data.clone()).map_err(GridError::from)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    pub version: u32,
    pub twin_id: u32,
    pub contract_id: u64,
    pub metadata: String,
    pub description: String,
    pub expiration: i64,
    pub signature_requirement: SignatureRequirement,
    pub workloads: Vec<Workload>,
}

impl Deployment {
    pub fn workload_with_id(&self, name: &str) -> Option<Workload> {
        self.workloads.iter().find(|w| w.name == name).cloned()
    }

    pub fn by_type(&self, kind: &str) -> Vec<Workload> {
        self.workloads
            .iter()
            .filter(|w| w.workload_type == kind)
            .cloned()
            .collect()
    }
}

impl Default for Deployment {
    fn default() -> Self {
        Self {
            version: 0,
            twin_id: 0,
            contract_id: 0,
            metadata: String::new(),
            description: String::new(),
            expiration: 0,
            signature_requirement: SignatureRequirement::default(),
            workloads: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentBuilder {
    pub version: u32,
    pub twin_id: u32,
    pub workloads: Vec<Workload>,
}

impl DeploymentBuilder {
    pub fn new(twin_id: u32) -> Self {
        Self {
            version: 0,
            twin_id,
            workloads: Vec::new(),
        }
    }

    pub fn add_workload(mut self, workload: Workload) -> Self {
        self.workloads.push(workload);
        self
    }

    pub fn build(self, contract_id: u64) -> Deployment {
        Deployment {
            version: self.version,
            twin_id: self.twin_id,
            contract_id,
            workloads: self.workloads,
            metadata: String::new(),
            description: String::new(),
            expiration: 0,
            signature_requirement: SignatureRequirement::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SignatureRequirement {
    #[serde(default)]
    pub requests: Vec<SignatureRequest>,
    #[serde(default)]
    pub weight_required: u32,
    #[serde(default)]
    pub signatures: Vec<String>,
    #[serde(default)]
    pub signature_style: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureRequest {
    pub twin_id: u32,
    #[serde(default)]
    pub required: bool,
    pub weight: u32,
}
