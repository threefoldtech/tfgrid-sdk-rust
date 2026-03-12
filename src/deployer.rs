//! Lightweight deployer facade for contract + deployment lifecycle wiring.

use std::sync::Arc;

use crate::{
    error::GridError,
    node::NodeClientGetter,
    subi::{self, SubstrateExt},
    workloads, zos,
};

pub struct Deployer {
    substrate: Arc<dyn SubstrateExt + Send + Sync>,
    nc_pool: Arc<dyn NodeClientGetter>,
}

impl Deployer {
    pub fn new(
        substrate: Arc<dyn SubstrateExt + Send + Sync>,
        nc_pool: Arc<dyn NodeClientGetter>,
    ) -> Self {
        Self { substrate, nc_pool }
    }

    pub fn deploy(
        &self,
        identity: &subi::Identity,
        deployment: &workloads::Deployment,
    ) -> Result<u64, GridError> {
        let node_id = deployment.node_id;
        let twin = self.substrate.get_node_twin(node_id)?;
        let metadata = deployment.generate_metadata();
        let hash = format!(
            "{}:{}:{}",
            deployment.name, node_id, deployment.solution_type
        );
        let public_ips = deployment.get_public_ip_count();
        let contract_id = self
            .substrate
            .create_node_contract(identity, node_id, &metadata, &hash, public_ips)?;

        let mut zos_dep = deployment.zos_deployment(twin)?;
        zos_dep.contract_id = contract_id;
        let client = self
            .nc_pool
            .get_node_client(self.substrate.as_ref(), node_id)?;
        client.deployment_put(zos_dep)?;
        Ok(contract_id)
    }

    pub fn cancel(&self, identity: &subi::Identity, contract_id: u64) -> Result<(), GridError> {
        self.substrate.cancel_contract(identity, contract_id)?;
        let contract = self.substrate.get_contract(contract_id)?;
        if let Some(node_id) = contract.contract_type.node_id() {
            let client = self
                .nc_pool
                .get_node_client(self.substrate.as_ref(), node_id)?;
            let _ = client.deployment_delete(contract_id);
        }
        Ok(())
    }

    pub fn load_node_deployment(
        &self,
        node_id: u32,
        contract_id: u64,
    ) -> Result<zos::Deployment, GridError> {
        let client = self
            .nc_pool
            .get_node_client(self.substrate.as_ref(), node_id)?;
        client.deployment_get(contract_id)
    }
}

impl workloads::Deployment {
    pub fn get_public_ip_count(&self) -> u32 {
        self.vms
            .iter()
            .map(|vm| u32::from(vm.public_ip || vm.public_ip6))
            .sum::<u32>()
            + self.zdbs.iter().map(|_| 0u32).sum::<u32>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{node::MockNodeClientGetter, subi};

    #[test]
    fn deploy_creates_contract_and_uploads_deployment() {
        let node_id = 7u32;
        let identity = subi::Identity::from("alice");

        let mut substrate = subi::MockSubstrate::new();
        substrate.add_node(subi::Node {
            id: node_id,
            certification: subi::Certification {
                is_certified: false,
            },
            resources: subi::NodeResources::default(),
        });
        let substrate = Arc::new(substrate);

        let nc_pool = Arc::new(MockNodeClientGetter::new());
        let deployer = Deployer::new(
            substrate.clone() as Arc<dyn SubstrateExt + Send + Sync>,
            nc_pool.clone() as Arc<dyn NodeClientGetter>,
        );

        let deployment = workloads::Deployment::new(
            "dep1",
            node_id,
            "solution-a",
            None,
            "",
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        let contract_id = deployer.deploy(&identity, &deployment).expect("deploy");
        let contract = substrate.get_contract(contract_id).expect("contract");
        assert!(contract.is_created());
        assert_eq!(contract.contract_type.node_contract.node, node_id);
        assert_eq!(contract.contract_type.node_contract.public_ips_count, 0);

        let uploaded = deployer
            .load_node_deployment(node_id, contract_id)
            .expect("uploaded deployment");
        assert_eq!(uploaded.contract_id, contract_id);
        assert_eq!(uploaded.metadata, deployment.generate_metadata());
    }

    #[test]
    fn cancel_marks_contract_deleted_and_removes_node_deployment() {
        let node_id = 8u32;
        let identity = subi::Identity::from("bob");

        let mut substrate = subi::MockSubstrate::new();
        substrate.add_node(subi::Node {
            id: node_id,
            certification: subi::Certification {
                is_certified: false,
            },
            resources: subi::NodeResources::default(),
        });
        let substrate = Arc::new(substrate);

        let nc_pool = Arc::new(MockNodeClientGetter::new());
        let deployer = Deployer::new(
            substrate.clone() as Arc<dyn SubstrateExt + Send + Sync>,
            nc_pool.clone() as Arc<dyn NodeClientGetter>,
        );

        let deployment = workloads::Deployment::new(
            "dep2",
            node_id,
            "solution-b",
            None,
            "",
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        let contract_id = deployer.deploy(&identity, &deployment).expect("deploy");
        deployer.cancel(&identity, contract_id).expect("cancel");

        let contract = substrate.get_contract(contract_id).expect("contract");
        assert!(contract.is_deleted());

        let load = deployer.load_node_deployment(node_id, contract_id);
        assert!(matches!(load, Err(GridError::NotFound(_))));
    }
}
