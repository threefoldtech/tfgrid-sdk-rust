//! Node client abstraction used by state and deployer flows.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::{error::GridError, subi::SubstrateExt, zos};

pub type NodeResult<T> = Result<T, GridError>;

pub trait NodeClient: Send + Sync {
    fn deployment_list(&self) -> NodeResult<Vec<zos::Deployment>>;
    fn deployment_get(&self, contract_id: u64) -> NodeResult<zos::Deployment>;
    fn deployment_put(&self, deployment: zos::Deployment) -> NodeResult<()>;
    fn deployment_delete(&self, contract_id: u64) -> NodeResult<()>;
    fn node_id(&self) -> u32;

    fn get_node_endpoint(&self) -> NodeResult<String> {
        Ok(String::new())
    }

    fn is_node_up(&self) -> NodeResult<()> {
        Ok(())
    }
}

pub trait NodeClientGetter: Send + Sync {
    fn get_node_client(
        &self,
        substrate: &dyn SubstrateExt,
        node_id: u32,
    ) -> NodeResult<Arc<dyn NodeClient>>;
}

#[derive(Default, Debug)]
struct MockNodeClientState {
    deployments: HashMap<u64, zos::Deployment>,
}

#[derive(Debug)]
pub struct MockNodeClient {
    node_id: u32,
    state: Arc<Mutex<MockNodeClientState>>,
}

impl MockNodeClient {
    pub fn new(node_id: u32) -> Self {
        Self {
            node_id,
            state: Arc::new(Mutex::new(MockNodeClientState::default())),
        }
    }

    fn with_shared(node_id: u32, state: Arc<Mutex<MockNodeClientState>>) -> Self {
        Self { node_id, state }
    }

    pub fn insert_deployment(&self, deployment: zos::Deployment) {
        let contract_id = deployment.contract_id;
        self.state
            .lock()
            .expect("lock poisoned")
            .deployments
            .insert(contract_id, deployment);
    }

    pub fn deployments(&self) -> Vec<zos::Deployment> {
        self.state
            .lock()
            .expect("lock poisoned")
            .deployments
            .values()
            .cloned()
            .collect()
    }
}

impl NodeClient for MockNodeClient {
    fn deployment_list(&self) -> NodeResult<Vec<zos::Deployment>> {
        Ok(self.deployments())
    }

    fn deployment_get(&self, contract_id: u64) -> NodeResult<zos::Deployment> {
        self.state
            .lock()
            .expect("lock poisoned")
            .deployments
            .get(&contract_id)
            .cloned()
            .ok_or_else(|| GridError::NotFound(format!("deployment {contract_id}")))
    }

    fn deployment_put(&self, deployment: zos::Deployment) -> NodeResult<()> {
        if deployment.contract_id == 0 {
            return Err(GridError::validation("contract id is required"));
        }
        self.insert_deployment(deployment);
        Ok(())
    }

    fn deployment_delete(&self, contract_id: u64) -> NodeResult<()> {
        self.state
            .lock()
            .expect("lock poisoned")
            .deployments
            .remove(&contract_id);
        Ok(())
    }

    fn node_id(&self) -> u32 {
        self.node_id
    }
}

#[derive(Default)]
pub struct MockNodeClientGetter {
    clients: Mutex<HashMap<u32, Arc<Mutex<MockNodeClientState>>>>,
}

impl MockNodeClientGetter {
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
        }
    }

    pub fn client(&self, node_id: u32) -> Option<Arc<dyn NodeClient>> {
        self.clients
            .lock()
            .ok()?
            .get(&node_id)
            .map(|state: &Arc<Mutex<MockNodeClientState>>| {
                Arc::new(MockNodeClient::with_shared(node_id, state.clone())) as Arc<dyn NodeClient>
            })
    }

    pub fn insert_deployment(&self, node_id: u32, deployment: zos::Deployment) -> NodeResult<()> {
        let client = self.ensure_node_client(node_id)?;
        client.insert_deployment(deployment);
        Ok(())
    }

    fn ensure_node_client(&self, node_id: u32) -> NodeResult<Arc<MockNodeClient>> {
        let mut clients = self.clients.lock().expect("lock poisoned");
        let state = clients
            .entry(node_id)
            .or_insert_with(|| Arc::new(Mutex::new(MockNodeClientState::default())));
        let shared = state.clone();
        Ok(Arc::new(MockNodeClient::with_shared(node_id, shared)))
    }
}

impl NodeClientGetter for MockNodeClientGetter {
    fn get_node_client(
        &self,
        substrate: &dyn SubstrateExt,
        node_id: u32,
    ) -> NodeResult<Arc<dyn NodeClient>> {
        let _ = substrate.get_node(node_id)?;
        let client = self.ensure_node_client(node_id)?;
        Ok(Arc::new(MockNodeClient::with_shared(node_id, {
            client.state.clone()
        })))
    }
}
