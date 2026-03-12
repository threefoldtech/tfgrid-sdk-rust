//! Substrate-facing data types and a lightweight Rust trait used by the SDK modules.

use crate::error::GridError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

pub const DEFAULT_PRICING_POLICY_ID: u32 = 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Policy {
    pub value: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PricingPolicy {
    pub id: u32,
    pub su: Policy,
    pub cu: Policy,
    pub ipu: Policy,
    #[serde(rename = "unique_name")]
    pub unique_name: Policy,
    #[serde(rename = "dedicated_nodes_discount")]
    pub dedicated_nodes_discount: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Balance {
    pub free: u128,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Contract {
    pub contract_id: u64,
    pub state: ContractState,
    pub contract_type: ContractType,
}

impl Contract {
    pub fn is_deleted(&self) -> bool {
        self.state.is_deleted
    }

    pub fn is_created(&self) -> bool {
        self.state.is_created
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContractState {
    pub is_created: bool,
    pub is_deleted: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContractType {
    #[serde(rename = "is_name")]
    pub is_name_contract: bool,
    #[serde(rename = "is_node")]
    pub is_node_contract: bool,
    #[serde(rename = "is_rent")]
    pub is_rent_contract: bool,
    #[serde(default)]
    pub node_contract: NodeContract,
    #[serde(default)]
    pub rent_contract: RentContract,
}

impl ContractType {
    pub fn kind(&self) -> ContractTypeKind {
        if self.is_name_contract {
            ContractTypeKind::Name
        } else if self.is_rent_contract {
            ContractTypeKind::Rent
        } else {
            ContractTypeKind::Node
        }
    }

    pub fn node_id(&self) -> Option<u32> {
        if self.is_node_contract {
            Some(self.node_contract.node)
        } else if self.is_rent_contract {
            Some(self.rent_contract.node)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractTypeKind {
    Name,
    Node,
    Rent,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeContract {
    pub node: u32,
    #[serde(rename = "public_ips_count")]
    pub public_ips_count: u32,
    #[serde(default)]
    pub deployment_data: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RentContract {
    pub node: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Certification {
    pub is_certified: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Node {
    pub id: u32,
    pub certification: Certification,
    pub resources: NodeResources,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeResources {
    pub cru: u64,
    pub mru: u64,
    pub hru: u64,
    pub sru: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContractPaymentState {
    pub last_updated_seconds: u64,
    pub standard_overdraft: u128,
    pub additional_overdraft: u128,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContractBillingInfo {
    pub amount_unbilled: u128,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeContractResources {
    pub used: ResourceUsed,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceUsed {
    pub cru: u64,
    pub mru: u64,
    pub hru: u64,
    pub sru: u64,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Identity(pub String);

impl Identity {
    pub fn address(&self) -> &str {
        &self.0
    }
}

/// Trait consumed by calculator/state/deployer for substrate operations used in this crate.
pub trait SubstrateExt {
    fn get_tft_billing_rate(&self) -> Result<u64, GridError>;
    fn get_pricing_policy(&self, policy_id: u32) -> Result<PricingPolicy, GridError>;
    fn get_balance(&self, identity: &str) -> Result<Balance, GridError>;
    fn get_contract(&self, contract_id: u64) -> Result<Contract, GridError>;
    fn get_contract_payment_state(
        &self,
        contract_id: u64,
    ) -> Result<ContractPaymentState, GridError>;
    fn get_contract_billing_info(&self, contract_id: u64)
    -> Result<ContractBillingInfo, GridError>;
    fn get_node(&self, node_id: u32) -> Result<Node, GridError>;
    fn get_node_contract_resources(
        &self,
        contract_id: u64,
    ) -> Result<NodeContractResources, GridError>;
    fn get_node_rent_contract(&self, node_id: u32) -> Result<u64, GridError>;
    fn get_node_contracts(&self, node_id: u32) -> Result<Vec<u64>, GridError>;
    fn get_node_twin(&self, node_id: u32) -> Result<u32, GridError>;
    fn get_dedicated_node_price(&self, node_id: u32) -> Result<u64, GridError>;
    fn get_contract_with_hash(
        &self,
        identity: &Identity,
        node_id: u32,
        hash: &str,
    ) -> Result<u64, GridError>;
    fn get_contract_id_by_name_registration(&self, name: &str) -> Result<u64, GridError> {
        Err(GridError::NotFound(format!(
            "name contract {name} not found"
        )))
    }
    fn get_contract_broadcast_data(&self, node_id: u32) -> Result<String, GridError> {
        let c = self.get_node(node_id)?;
        Ok(format!("node:{}:{}", c.id, c.certification.is_certified))
    }

    fn new_identity_from_sr25519_phrase(&self, phrase: &str) -> Result<Identity, GridError> {
        Ok(Identity(phrase.to_owned()))
    }

    fn ensure_contract_canceled(&self, _contract_id: u64) -> Result<(), GridError> {
        Ok(())
    }

    fn create_node_contract(
        &self,
        _identity: &Identity,
        _node: u32,
        _deployment_data: &str,
        _hash: &str,
        _public_ips: u32,
    ) -> Result<u64, GridError> {
        Err(GridError::backend(
            "create_node_contract is not implemented",
        ))
    }

    fn cancel_contract(&self, _identity: &Identity, _contract_id: u64) -> Result<(), GridError> {
        Ok(())
    }
}

/// In-memory mock implementation for unit tests and docs.
#[derive(Debug, Default)]
pub struct MockSubstrate {
    pub pricing_policy: PricingPolicy,
    pub tft_rate: u64,
    pub contracts: Mutex<HashMap<u64, Contract>>,
    pub nodes: Mutex<HashMap<u32, Node>>,
    pub node_contracts: Mutex<HashMap<u32, Vec<u64>>>,
    pub contract_resources: Mutex<HashMap<u64, NodeContractResources>>,
    pub contract_payment: Mutex<HashMap<u64, ContractPaymentState>>,
    pub contract_billing: Mutex<HashMap<u64, ContractBillingInfo>>,
    pub contract_hashes: Mutex<HashMap<String, u64>>,
    pub name_contracts: Mutex<HashMap<String, u64>>,
    pub balances: Mutex<HashMap<String, Balance>>,
    next_contract_id: AtomicU64,
}

impl MockSubstrate {
    #[allow(clippy::too_many_arguments)]
    pub fn new() -> Self {
        Self {
            pricing_policy: PricingPolicy {
                id: DEFAULT_PRICING_POLICY_ID,
                su: Policy { value: 50_000 },
                cu: Policy { value: 100_000 },
                ipu: Policy { value: 40_000 },
                unique_name: Policy { value: 300_000 },
                dedicated_nodes_discount: 20,
            },
            tft_rate: 5,
            contracts: Mutex::new(HashMap::new()),
            nodes: Mutex::new(HashMap::new()),
            node_contracts: Mutex::new(HashMap::new()),
            contract_resources: Mutex::new(HashMap::new()),
            contract_payment: Mutex::new(HashMap::new()),
            contract_billing: Mutex::new(HashMap::new()),
            contract_hashes: Mutex::new(HashMap::new()),
            name_contracts: Mutex::new(HashMap::new()),
            balances: Mutex::new(HashMap::new()),
            next_contract_id: AtomicU64::new(1),
        }
    }

    pub fn add_node(&mut self, node: Node) {
        self.nodes
            .lock()
            .expect("lock poisoned")
            .insert(node.id, node);
    }

    pub fn add_contract(&mut self, contract: Contract) {
        if let Some(node_id) = contract.contract_type.node_id() {
            self.node_contracts
                .lock()
                .expect("lock poisoned")
                .entry(node_id)
                .or_default()
                .push(contract.contract_id);
        }
        let contract_id = contract.contract_id;
        self.contracts
            .lock()
            .expect("lock poisoned")
            .insert(contract_id, contract);
        let current = self.next_contract_id.load(Ordering::Relaxed);
        if contract_id >= current {
            self.next_contract_id
                .store(contract_id.saturating_add(1), Ordering::Relaxed);
        }
    }

    pub fn add_contract_payment(&mut self, contract_id: u64, state: ContractPaymentState) {
        self.contract_payment
            .lock()
            .expect("lock poisoned")
            .insert(contract_id, state);
    }

    pub fn add_contract_resources(&mut self, contract_id: u64, resources: NodeContractResources) {
        self.contract_resources
            .lock()
            .expect("lock poisoned")
            .insert(contract_id, resources);
    }

    pub fn add_contract_billing_info(&mut self, contract_id: u64, info: ContractBillingInfo) {
        self.contract_billing
            .lock()
            .expect("lock poisoned")
            .insert(contract_id, info);
    }

    pub fn set_balance<S: Into<String>>(&mut self, identity: S, balance: Balance) {
        self.balances
            .lock()
            .expect("lock poisoned")
            .insert(identity.into(), balance);
    }

    pub fn set_contract_hash<S: Into<String>>(&mut self, hash: S, contract_id: u64) {
        self.contract_hashes
            .lock()
            .expect("lock poisoned")
            .insert(hash.into(), contract_id);
    }

    pub fn set_contract_name<S: Into<String>>(&mut self, name: S, contract_id: u64) {
        self.name_contracts
            .lock()
            .expect("lock poisoned")
            .insert(name.into(), contract_id);
    }
}

impl SubstrateExt for MockSubstrate {
    fn get_tft_billing_rate(&self) -> Result<u64, GridError> {
        Ok(self.tft_rate)
    }

    fn get_pricing_policy(&self, policy_id: u32) -> Result<PricingPolicy, GridError> {
        if self.pricing_policy.id == policy_id {
            Ok(self.pricing_policy.clone())
        } else {
            Err(GridError::NotFound(format!("pricing policy {policy_id}")))
        }
    }

    fn get_balance(&self, identity: &str) -> Result<Balance, GridError> {
        self.balances
            .lock()
            .expect("lock poisoned")
            .get(identity)
            .cloned()
            .ok_or_else(|| GridError::NotFound(format!("identity {identity}")))
    }

    fn get_contract(&self, contract_id: u64) -> Result<Contract, GridError> {
        self.contracts
            .lock()
            .expect("lock poisoned")
            .get(&contract_id)
            .cloned()
            .ok_or_else(|| GridError::NotFound(format!("contract {contract_id}")))
    }

    fn get_contract_payment_state(
        &self,
        contract_id: u64,
    ) -> Result<ContractPaymentState, GridError> {
        self.contract_payment
            .lock()
            .expect("lock poisoned")
            .get(&contract_id)
            .cloned()
            .ok_or_else(|| GridError::NotFound(format!("contract payment state {contract_id}")))
    }

    fn get_contract_billing_info(
        &self,
        contract_id: u64,
    ) -> Result<ContractBillingInfo, GridError> {
        self.contract_billing
            .lock()
            .expect("lock poisoned")
            .get(&contract_id)
            .cloned()
            .ok_or_else(|| GridError::NotFound(format!("contract billing info {contract_id}")))
    }

    fn get_node(&self, node_id: u32) -> Result<Node, GridError> {
        self.nodes
            .lock()
            .expect("lock poisoned")
            .get(&node_id)
            .cloned()
            .ok_or_else(|| GridError::NotFound(format!("node {node_id}")))
    }

    fn get_node_contract_resources(
        &self,
        contract_id: u64,
    ) -> Result<NodeContractResources, GridError> {
        self.contract_resources
            .lock()
            .expect("lock poisoned")
            .get(&contract_id)
            .cloned()
            .ok_or_else(|| GridError::NotFound(format!("node contract resources {contract_id}")))
    }

    fn get_node_rent_contract(&self, node_id: u32) -> Result<u64, GridError> {
        self.nodes
            .lock()
            .expect("lock poisoned")
            .get(&node_id)
            .and_then(|node| {
                self.contracts
                    .lock()
                    .expect("lock poisoned")
                    .values()
                    .find(|c| c.contract_type.rent_contract.node == node.id && c.state.is_created)
                    .map(|contract| contract.contract_id)
            })
            .ok_or_else(|| GridError::NotFound(format!("rent contract {node_id}")))
    }

    fn get_node_contracts(&self, node_id: u32) -> Result<Vec<u64>, GridError> {
        Ok(self
            .node_contracts
            .lock()
            .expect("lock poisoned")
            .get(&node_id)
            .cloned()
            .unwrap_or_default())
    }

    fn get_node_twin(&self, node_id: u32) -> Result<u32, GridError> {
        self.nodes
            .lock()
            .expect("lock poisoned")
            .contains_key(&node_id)
            .then_some(node_id)
            .ok_or_else(|| GridError::NotFound(format!("node twin {node_id}")))
    }

    fn get_dedicated_node_price(&self, node_id: u32) -> Result<u64, GridError> {
        self.nodes
            .lock()
            .expect("lock poisoned")
            .contains_key(&node_id)
            .then_some(0)
            .ok_or_else(|| GridError::NotFound(format!("node {node_id}")))
    }

    fn get_contract_with_hash(
        &self,
        _identity: &Identity,
        node_id: u32,
        hash: &str,
    ) -> Result<u64, GridError> {
        let key = format!("{node_id}:{hash}");
        self.contract_hashes
            .lock()
            .expect("lock poisoned")
            .get(&key)
            .copied()
            .ok_or_else(|| GridError::NotFound(format!("contract hash {hash}")))
    }

    fn get_contract_id_by_name_registration(&self, name: &str) -> Result<u64, GridError> {
        self.name_contracts
            .lock()
            .expect("lock poisoned")
            .get(name)
            .copied()
            .ok_or_else(|| GridError::NotFound(format!("name contract {name} not found")))
    }

    fn ensure_contract_canceled(&self, contract_id: u64) -> Result<(), GridError> {
        let mut contracts = self.contracts.lock().expect("lock poisoned");
        let contract = contracts
            .get_mut(&contract_id)
            .ok_or_else(|| GridError::NotFound(format!("contract {contract_id}")))?;
        contract.state.is_created = false;
        contract.state.is_deleted = true;
        Ok(())
    }

    fn create_node_contract(
        &self,
        _identity: &Identity,
        node: u32,
        deployment_data: &str,
        hash: &str,
        public_ips: u32,
    ) -> Result<u64, GridError> {
        if !self
            .nodes
            .lock()
            .expect("lock poisoned")
            .contains_key(&node)
        {
            return Err(GridError::NotFound(format!("node {node}")));
        }

        let contract_id = self.next_contract_id.fetch_add(1, Ordering::Relaxed);
        let contract = Contract {
            contract_id,
            state: ContractState {
                is_created: true,
                is_deleted: false,
            },
            contract_type: ContractType {
                is_name_contract: false,
                is_node_contract: true,
                is_rent_contract: false,
                node_contract: NodeContract {
                    node,
                    public_ips_count: public_ips,
                    deployment_data: deployment_data.to_string(),
                },
                rent_contract: RentContract::default(),
            },
        };
        self.contracts
            .lock()
            .expect("lock poisoned")
            .insert(contract_id, contract);
        self.node_contracts
            .lock()
            .expect("lock poisoned")
            .entry(node)
            .or_default()
            .push(contract_id);
        self.contract_hashes
            .lock()
            .expect("lock poisoned")
            .insert(format!("{node}:{hash}"), contract_id);
        Ok(contract_id)
    }

    fn cancel_contract(&self, _identity: &Identity, contract_id: u64) -> Result<(), GridError> {
        self.ensure_contract_canceled(contract_id)
    }
}

impl From<&str> for Identity {
    fn from(value: &str) -> Self {
        Identity(value.to_owned())
    }
}
