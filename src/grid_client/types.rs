use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::zos;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentOutcome {
    pub node_id: u32,
    pub node_twin_id: u32,
    pub network_name: String,
    pub network_contract_id: u64,
    pub vm_name: String,
    pub vm_contract_id: u64,
    pub vm_ip: String,
    pub mycelium_ip: String,
    pub public_ipv4: String,
    pub public_ipv6: String,
    pub console_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRequirements {
    pub min_cru: u64,
    pub min_memory_bytes: u64,
    pub min_rootfs_bytes: u64,
}

impl Default for NodeRequirements {
    fn default() -> Self {
        Self {
            min_cru: 1,
            min_memory_bytes: 1024 * zos::MEGABYTE,
            min_rootfs_bytes: 10 * zos::GIGABYTE,
        }
    }
}

impl NodeRequirements {
    pub fn builder() -> NodeRequirementsBuilder {
        NodeRequirementsBuilder {
            requirements: Self::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodePlacement {
    Auto(NodeRequirements),
    Fixed { node_id: u32, node_twin_id: u32 },
}

impl Default for NodePlacement {
    fn default() -> Self {
        Self::Auto(NodeRequirements::default())
    }
}

impl NodePlacement {
    pub fn auto() -> Self {
        Self::Auto(NodeRequirements::default())
    }

    pub fn auto_with(requirements: NodeRequirements) -> Self {
        Self::Auto(requirements)
    }

    pub fn fixed(node_id: u32, node_twin_id: u32) -> Self {
        Self::Fixed {
            node_id,
            node_twin_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkLightSpec {
    pub name: Option<String>,
    pub subnet: Option<String>,
    pub mycelium_key: Option<Vec<u8>>,
}

impl NetworkLightSpec {
    pub fn builder() -> NetworkLightSpecBuilder {
        NetworkLightSpecBuilder {
            spec: Self::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingNetworkSpec {
    pub name: String,
    pub ip: String,
}

impl ExistingNetworkSpec {
    pub fn new(name: impl Into<String>, ip: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ip: ip.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkTarget {
    Create(NetworkLightSpec),
    Existing(ExistingNetworkSpec),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmLightMount {
    pub name: String,
    pub mountpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmLightSpec {
    pub name: Option<String>,
    pub flist: String,
    pub cpu: u8,
    pub memory_bytes: u64,
    pub rootfs_size_bytes: u64,
    pub entrypoint: String,
    pub env: HashMap<String, String>,
    pub mounts: Vec<VmLightMount>,
    pub volumes: Vec<VolumeMountSpec>,
    pub corex: bool,
    pub gpu: Vec<String>,
    pub mycelium_seed: Option<Vec<u8>>,
}

impl Default for VmLightSpec {
    fn default() -> Self {
        Self {
            name: None,
            flist: "https://hub.grid.tf/tf-official-apps/base:latest.flist".to_string(),
            cpu: 1,
            memory_bytes: 1024 * zos::MEGABYTE,
            rootfs_size_bytes: 10 * zos::GIGABYTE,
            entrypoint: "/sbin/zinit init".to_string(),
            env: HashMap::new(),
            mounts: Vec::new(),
            volumes: Vec::new(),
            corex: false,
            gpu: Vec::new(),
            mycelium_seed: None,
        }
    }
}

impl VmLightSpec {
    pub fn builder() -> VmLightSpecBuilder {
        VmLightSpecBuilder {
            spec: Self::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmLightDeployment {
    pub placement: NodePlacement,
    pub network: NetworkTarget,
    pub vm: VmLightSpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FullNetworkSpec {
    pub name: Option<String>,
    pub ip_range: Option<String>,
    pub subnet: Option<String>,
    pub mycelium_key: Option<Vec<u8>>,
    pub wireguard_private_key: Option<String>,
    pub wireguard_listen_port: Option<u16>,
}

impl FullNetworkSpec {
    pub fn builder() -> FullNetworkSpecBuilder {
        FullNetworkSpecBuilder {
            spec: Self::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FullNetworkTarget {
    Create(FullNetworkSpec),
    Existing(ExistingNetworkSpec),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeMountSpec {
    pub name: String,
    pub size_bytes: u64,
    pub mountpoint: String,
    pub description: String,
}

impl VolumeMountSpec {
    pub fn new(name: impl Into<String>, size_bytes: u64, mountpoint: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            size_bytes,
            mountpoint: mountpoint.into(),
            description: String::new(),
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmSpec {
    pub name: Option<String>,
    pub flist: String,
    pub cpu: u8,
    pub memory_bytes: u64,
    pub rootfs_size_bytes: u64,
    pub entrypoint: String,
    pub env: HashMap<String, String>,
    pub volumes: Vec<VolumeMountSpec>,
    pub planetary: bool,
    pub public_ipv4: bool,
    pub public_ipv6: bool,
    pub corex: bool,
    pub gpu: Vec<String>,
    pub mycelium_seed: Option<Vec<u8>>,
}

impl Default for VmSpec {
    fn default() -> Self {
        Self {
            name: None,
            flist: "https://hub.grid.tf/tf-official-apps/base:latest.flist".to_string(),
            cpu: 1,
            memory_bytes: 1024 * zos::MEGABYTE,
            rootfs_size_bytes: 10 * zos::GIGABYTE,
            entrypoint: "/sbin/zinit init".to_string(),
            env: HashMap::new(),
            volumes: Vec::new(),
            planetary: false,
            public_ipv4: false,
            public_ipv6: false,
            corex: false,
            gpu: Vec::new(),
            mycelium_seed: None,
        }
    }
}

impl VmSpec {
    pub fn builder() -> VmSpecBuilder {
        VmSpecBuilder {
            spec: Self::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmDeployment {
    pub placement: NodePlacement,
    pub network: FullNetworkTarget,
    pub vm: VmSpec,
}

#[derive(Debug, Clone)]
pub struct VmLightDeploymentBuilder {
    request: VmLightDeployment,
}

#[derive(Debug, Clone)]
pub struct VmDeploymentBuilder {
    request: VmDeployment,
}

#[derive(Debug, Clone)]
pub struct NodeRequirementsBuilder {
    requirements: NodeRequirements,
}

#[derive(Debug, Clone)]
pub struct NetworkLightSpecBuilder {
    spec: NetworkLightSpec,
}

#[derive(Debug, Clone)]
pub struct FullNetworkSpecBuilder {
    spec: FullNetworkSpec,
}

#[derive(Debug, Clone)]
pub struct VmLightSpecBuilder {
    spec: VmLightSpec,
}

#[derive(Debug, Clone)]
pub struct VmSpecBuilder {
    spec: VmSpec,
}

impl VmLightDeployment {
    pub fn builder() -> VmLightDeploymentBuilder {
        VmLightDeploymentBuilder {
            request: Self {
                placement: NodePlacement::default(),
                network: NetworkTarget::Create(NetworkLightSpec::default()),
                vm: VmLightSpec::default(),
            },
        }
    }
}

impl VmLightDeploymentBuilder {
    pub fn auto(mut self) -> Self {
        self.request.placement = NodePlacement::auto();
        self
    }

    pub fn auto_with(mut self, requirements: NodeRequirements) -> Self {
        self.request.placement = NodePlacement::auto_with(requirements);
        self
    }

    pub fn fixed_node(mut self, node_id: u32, node_twin_id: u32) -> Self {
        self.request.placement = NodePlacement::fixed(node_id, node_twin_id);
        self
    }

    pub fn placement(mut self, placement: NodePlacement) -> Self {
        self.request.placement = placement;
        self
    }

    pub fn create_network(mut self, network: NetworkLightSpec) -> Self {
        self.request.network = NetworkTarget::Create(network);
        self
    }

    pub fn existing_network(mut self, name: impl Into<String>, ip: impl Into<String>) -> Self {
        self.request.network = NetworkTarget::Existing(ExistingNetworkSpec::new(name, ip));
        self
    }

    pub fn network(mut self, network: NetworkTarget) -> Self {
        self.request.network = network;
        self
    }

    pub fn vm(mut self, vm: VmLightSpec) -> Self {
        self.request.vm = vm;
        self
    }

    pub fn build(self) -> VmLightDeployment {
        self.request
    }
}

impl VmDeployment {
    pub fn builder() -> VmDeploymentBuilder {
        VmDeploymentBuilder {
            request: Self {
                placement: NodePlacement::default(),
                network: FullNetworkTarget::Create(FullNetworkSpec::default()),
                vm: VmSpec::default(),
            },
        }
    }
}

impl VmDeploymentBuilder {
    pub fn auto(mut self) -> Self {
        self.request.placement = NodePlacement::auto();
        self
    }

    pub fn auto_with(mut self, requirements: NodeRequirements) -> Self {
        self.request.placement = NodePlacement::auto_with(requirements);
        self
    }

    pub fn fixed_node(mut self, node_id: u32, node_twin_id: u32) -> Self {
        self.request.placement = NodePlacement::fixed(node_id, node_twin_id);
        self
    }

    pub fn placement(mut self, placement: NodePlacement) -> Self {
        self.request.placement = placement;
        self
    }

    pub fn create_network(mut self, network: FullNetworkSpec) -> Self {
        self.request.network = FullNetworkTarget::Create(network);
        self
    }

    pub fn existing_network(mut self, name: impl Into<String>, ip: impl Into<String>) -> Self {
        self.request.network = FullNetworkTarget::Existing(ExistingNetworkSpec::new(name, ip));
        self
    }

    pub fn network(mut self, network: FullNetworkTarget) -> Self {
        self.request.network = network;
        self
    }

    pub fn vm(mut self, vm: VmSpec) -> Self {
        self.request.vm = vm;
        self
    }

    pub fn build(self) -> VmDeployment {
        self.request
    }
}

impl NodeRequirementsBuilder {
    pub fn min_cru(mut self, min_cru: u64) -> Self {
        self.requirements.min_cru = min_cru;
        self
    }

    pub fn min_memory_bytes(mut self, min_memory_bytes: u64) -> Self {
        self.requirements.min_memory_bytes = min_memory_bytes;
        self
    }

    pub fn min_rootfs_bytes(mut self, min_rootfs_bytes: u64) -> Self {
        self.requirements.min_rootfs_bytes = min_rootfs_bytes;
        self
    }

    pub fn build(self) -> NodeRequirements {
        self.requirements
    }
}

impl NetworkLightSpecBuilder {
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.spec.name = Some(name.into());
        self
    }

    pub fn subnet(mut self, subnet: impl Into<String>) -> Self {
        self.spec.subnet = Some(subnet.into());
        self
    }

    pub fn mycelium_key(mut self, mycelium_key: Vec<u8>) -> Self {
        self.spec.mycelium_key = Some(mycelium_key);
        self
    }

    pub fn build(self) -> NetworkLightSpec {
        self.spec
    }
}

impl FullNetworkSpecBuilder {
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.spec.name = Some(name.into());
        self
    }

    pub fn ip_range(mut self, ip_range: impl Into<String>) -> Self {
        self.spec.ip_range = Some(ip_range.into());
        self
    }

    pub fn subnet(mut self, subnet: impl Into<String>) -> Self {
        self.spec.subnet = Some(subnet.into());
        self
    }

    pub fn mycelium_key(mut self, mycelium_key: Vec<u8>) -> Self {
        self.spec.mycelium_key = Some(mycelium_key);
        self
    }

    pub fn wireguard_private_key(mut self, wireguard_private_key: impl Into<String>) -> Self {
        self.spec.wireguard_private_key = Some(wireguard_private_key.into());
        self
    }

    pub fn wireguard_listen_port(mut self, wireguard_listen_port: u16) -> Self {
        self.spec.wireguard_listen_port = Some(wireguard_listen_port);
        self
    }

    pub fn build(self) -> FullNetworkSpec {
        self.spec
    }
}

impl VmLightSpecBuilder {
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.spec.name = Some(name.into());
        self
    }

    pub fn flist(mut self, flist: impl Into<String>) -> Self {
        self.spec.flist = flist.into();
        self
    }

    pub fn cpu(mut self, cpu: u8) -> Self {
        self.spec.cpu = cpu;
        self
    }

    pub fn memory_bytes(mut self, memory_bytes: u64) -> Self {
        self.spec.memory_bytes = memory_bytes;
        self
    }

    pub fn rootfs_size_bytes(mut self, rootfs_size_bytes: u64) -> Self {
        self.spec.rootfs_size_bytes = rootfs_size_bytes;
        self
    }

    pub fn entrypoint(mut self, entrypoint: impl Into<String>) -> Self {
        self.spec.entrypoint = entrypoint.into();
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.spec.env.insert(key.into(), value.into());
        self
    }

    pub fn mount(mut self, name: impl Into<String>, mountpoint: impl Into<String>) -> Self {
        self.spec.mounts.push(VmLightMount {
            name: name.into(),
            mountpoint: mountpoint.into(),
        });
        self
    }

    pub fn volume(mut self, volume: VolumeMountSpec) -> Self {
        self.spec.volumes.push(volume);
        self
    }

    pub fn corex(mut self, corex: bool) -> Self {
        self.spec.corex = corex;
        self
    }

    pub fn gpu(mut self, gpu: impl Into<String>) -> Self {
        self.spec.gpu.push(gpu.into());
        self
    }

    pub fn mycelium_seed(mut self, mycelium_seed: Vec<u8>) -> Self {
        self.spec.mycelium_seed = Some(mycelium_seed);
        self
    }

    pub fn build(self) -> VmLightSpec {
        self.spec
    }
}

impl VmSpecBuilder {
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.spec.name = Some(name.into());
        self
    }

    pub fn flist(mut self, flist: impl Into<String>) -> Self {
        self.spec.flist = flist.into();
        self
    }

    pub fn cpu(mut self, cpu: u8) -> Self {
        self.spec.cpu = cpu;
        self
    }

    pub fn memory_bytes(mut self, memory_bytes: u64) -> Self {
        self.spec.memory_bytes = memory_bytes;
        self
    }

    pub fn rootfs_size_bytes(mut self, rootfs_size_bytes: u64) -> Self {
        self.spec.rootfs_size_bytes = rootfs_size_bytes;
        self
    }

    pub fn entrypoint(mut self, entrypoint: impl Into<String>) -> Self {
        self.spec.entrypoint = entrypoint.into();
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.spec.env.insert(key.into(), value.into());
        self
    }

    pub fn volume(mut self, volume: VolumeMountSpec) -> Self {
        self.spec.volumes.push(volume);
        self
    }

    pub fn planetary(mut self, planetary: bool) -> Self {
        self.spec.planetary = planetary;
        self
    }

    pub fn public_ipv4(mut self, public_ipv4: bool) -> Self {
        self.spec.public_ipv4 = public_ipv4;
        self
    }

    pub fn public_ipv6(mut self, public_ipv6: bool) -> Self {
        self.spec.public_ipv6 = public_ipv6;
        self
    }

    pub fn corex(mut self, corex: bool) -> Self {
        self.spec.corex = corex;
        self
    }

    pub fn gpu(mut self, gpu: impl Into<String>) -> Self {
        self.spec.gpu.push(gpu.into());
        self
    }

    pub fn mycelium_seed(mut self, mycelium_seed: Vec<u8>) -> Self {
        self.spec.mycelium_seed = Some(mycelium_seed);
        self
    }

    pub fn build(self) -> VmSpec {
        self.spec
    }
}
