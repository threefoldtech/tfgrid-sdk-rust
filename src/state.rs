//! Grid state reconstruction helpers.

use std::net::IpAddr;
use std::{collections::HashMap, sync::Arc};

use crate::{
    error::GridError,
    node::NodeClientGetter,
    subi::{self, SubstrateExt},
    workloads, zos,
};

pub type ContractIds = Vec<u64>;

pub struct State {
    pub current_node_deployments: HashMap<u32, ContractIds>,
    pub networks: NetworkState,
    pub nc_pool: Arc<dyn NodeClientGetter>,
    pub substrate: Arc<dyn SubstrateExt + Send + Sync>,
}

impl State {
    pub fn new(
        nc_pool: Arc<dyn NodeClientGetter>,
        substrate: Arc<dyn SubstrateExt + Send + Sync>,
    ) -> Self {
        Self {
            current_node_deployments: HashMap::new(),
            networks: NetworkState::new(),
            nc_pool,
            substrate,
        }
    }

    pub fn store_contract_ids(&mut self, node_id: u32, contract_ids: &[u64]) {
        let entry = self.current_node_deployments.entry(node_id).or_default();
        for id in contract_ids {
            if !entry.contains(id) {
                entry.push(*id);
            }
        }
    }

    pub fn remove_contract_ids(&mut self, node_id: u32, contract_ids: &[u64]) {
        if let Some(vec) = self.current_node_deployments.get_mut(&node_id) {
            for contract_id in contract_ids {
                vec.retain(|x| x != contract_id);
            }
        }
    }

    pub fn load_disk_from_grid(
        &self,
        node_id: u32,
        name: &str,
        deployment_name: &str,
    ) -> Result<workloads::Disk, GridError> {
        let (wl, _) = self.get_workload_in_deployment(node_id, name, deployment_name)?;
        workloads::new_disk_from_workload(&wl)
    }

    pub fn load_volume_from_grid(
        &self,
        node_id: u32,
        name: &str,
        deployment_name: &str,
    ) -> Result<workloads::Volume, GridError> {
        let (wl, _) = self.get_workload_in_deployment(node_id, name, deployment_name)?;
        workloads::new_volume_from_workload(&wl)
    }

    pub fn load_vm_from_grid(
        &self,
        node_id: u32,
        name: &str,
        deployment_name: &str,
    ) -> Result<workloads::VM, GridError> {
        let (wl, dl) = self.get_workload_in_deployment(node_id, name, deployment_name)?;
        let mut vm = workloads::new_vm_from_workload(&wl, &dl, node_id)?;
        vm.node_id = node_id;
        Ok(vm)
    }

    pub fn load_vm_light_from_grid(
        &self,
        node_id: u32,
        name: &str,
        deployment_name: &str,
    ) -> Result<workloads::VMLight, GridError> {
        let (wl, dl) = self.get_workload_in_deployment(node_id, name, deployment_name)?;
        let mut vm = workloads::new_vm_light_from_workload(&wl, &dl, node_id)?;
        vm.node_id = node_id;
        Ok(vm)
    }

    pub fn load_zdb_from_grid(
        &self,
        node_id: u32,
        name: &str,
        deployment_name: &str,
    ) -> Result<workloads::ZDB, GridError> {
        let (wl, _) = self.get_workload_in_deployment(node_id, name, deployment_name)?;
        workloads::new_zdb_from_workload(&wl)
    }

    pub fn load_qsfs_from_grid(
        &self,
        node_id: u32,
        name: &str,
        deployment_name: &str,
    ) -> Result<workloads::QSFS, GridError> {
        let (wl, _) = self.get_workload_in_deployment(node_id, name, deployment_name)?;
        workloads::new_qsfs_from_workload(&wl)
    }

    pub fn load_gateway_name_from_grid(
        &self,
        node_id: u32,
        name: &str,
        deployment_name: &str,
    ) -> Result<workloads::GatewayNameProxy, GridError> {
        let (wl, dl) =
            self.get_workload_in_deployment(node_id, deployment_name, deployment_name)?;
        let meta = resolve_metadata(self.substrate.as_ref(), &dl)?;
        let deployment_data = workloads::parse_deployment_data(&meta)?;
        let mut gw = workloads::GatewayNameProxy::from_workload(&wl)?;
        gw.node_id = node_id;
        gw.contract_id = dl.contract_id;
        gw.solution_type = deployment_data.project_name;
        gw.node_deployment_id.insert(node_id, dl.contract_id);
        gw.name_contract_id = self.substrate.get_contract_id_by_name_registration(name)?;
        Ok(gw)
    }

    pub fn load_gateway_fqdn_from_grid(
        &self,
        node_id: u32,
        name: &str,
        deployment_name: &str,
    ) -> Result<workloads::GatewayFQDNProxy, GridError> {
        let (wl, dl) = self.get_workload_in_deployment(node_id, name, deployment_name)?;
        let meta = resolve_metadata(self.substrate.as_ref(), &dl)?;
        let deployment_data = workloads::parse_deployment_data(&meta)?;
        let mut gw = workloads::GatewayFQDNProxy::from_workload(&wl)?;
        gw.node_id = node_id;
        gw.contract_id = dl.contract_id;
        gw.solution_type = deployment_data.project_name;
        gw.node_deployment_id.insert(node_id, dl.contract_id);
        Ok(gw)
    }

    pub fn load_deployment_from_grid(
        &mut self,
        node_id: u32,
        name: &str,
    ) -> Result<workloads::Deployment, GridError> {
        let (_, dl) = self.get_workload_in_deployment(node_id, "", name)?;
        let mut output = workloads::new_deployment_from_zos_deployment(dl, node_id)?;
        if output.network_name.is_empty() {
            return Ok(output);
        }

        let network_load = self.load_network_from_grid(&output.network_name);
        if network_load.is_err() {
            self.load_network_light_from_grid(&output.network_name)?;
        }

        output.ip_range = self
            .networks
            .get_network(&output.network_name)
            .get_node_subnet(node_id);
        Ok(output)
    }

    pub fn load_network_from_grid(&mut self, name: &str) -> Result<workloads::ZNet, GridError> {
        let mut znet = workloads::ZNet {
            name: String::new(),
            description: String::new(),
            nodes: Vec::new(),
            ip_range: String::new(),
            add_wg_access: false,
            mycelium_keys: HashMap::new(),
            solution_type: String::new(),
            access_wg_config: String::new(),
            external_ip: None,
            external_sk: String::new(),
            public_node_id: 0,
            nodes_ip_range: HashMap::new(),
            node_deployment_id: HashMap::new(),
            wg_port: HashMap::new(),
            keys: HashMap::new(),
        };

        let mut node_deployments = HashMap::new();
        let mut z_nets = Vec::new();
        let mut public_node_endpoint = String::new();

        let sub = self.substrate.as_ref();
        for node_id in self.current_node_deployments.keys() {
            let node_client = self.nc_pool.get_node_client(sub, *node_id)?;
            let deployments = node_client.deployment_list()?;
            for deployment in deployments {
                let meta = resolve_metadata(sub, &deployment)?;
                let data = workloads::parse_deployment_data(&meta)?;

                for workload in deployment.workloads.iter() {
                    if workload.workload_type != zos::NETWORK_TYPE || workload.name != name {
                        continue;
                    }

                    let mut network = workloads::new_network_from_workload(workload, *node_id)?;
                    network.solution_type = data.project_name;
                    z_nets.push(network.clone());
                    node_deployments.insert(*node_id, deployment.contract_id);

                    if network.public_node_id == *node_id {
                        public_node_endpoint = node_client.get_node_endpoint()?;
                    }

                    break;
                }
            }
        }

        if z_nets.is_empty() {
            return Err(GridError::NotFound(format!("failed to get network {name}")));
        }

        let mut iter = z_nets.into_iter();
        if let Some(first) = iter.next() {
            znet = first;
        }

        for net in iter {
            znet.nodes.extend(net.nodes);
            znet.nodes_ip_range.extend(net.nodes_ip_range);
            znet.mycelium_keys.extend(net.mycelium_keys);
            znet.keys.extend(net.keys);
            znet.wg_port.extend(net.wg_port);
            if znet.ip_range.is_empty() {
                znet.ip_range = net.ip_range;
            }
        }

        znet.node_deployment_id = node_deployments;

        if znet.add_wg_access
            && !public_node_endpoint.is_empty()
            && let Some(ext_ip) = znet.external_ip.as_ref()
            && let Some(wg_ip) = compute_wg_ip(ext_ip)
        {
            let port = znet.wg_port.get(&znet.public_node_id).copied().unwrap_or(0);
            if port == 0 {
                return Err(GridError::validation(format!(
                    "missing wireguard port for network public node {}",
                    znet.public_node_id
                )));
            }
            let peer_key = znet
                .keys
                .get(&znet.public_node_id)
                .cloned()
                .unwrap_or_default();
            znet.access_wg_config = format_wg_config(
                &wg_ip,
                &znet.external_sk,
                &peer_key,
                &format!("{}:{}", public_node_endpoint, port),
                &znet.ip_range,
            );
        }

        self.networks
            .update_network_subnets(name, znet.nodes_ip_range.clone());
        Ok(znet)
    }

    pub fn load_network_light_from_grid(
        &mut self,
        name: &str,
    ) -> Result<workloads::ZNetLight, GridError> {
        let mut znet = workloads::ZNetLight {
            name: String::new(),
            description: String::new(),
            nodes: Vec::new(),
            solution_type: String::new(),
            ip_range: String::new(),
            nodes_ip_range: HashMap::new(),
            node_deployment_id: HashMap::new(),
            public_node_id: 0,
            mycelium_keys: HashMap::new(),
        };

        let mut node_deployments = HashMap::new();
        let mut z_nets = Vec::new();

        let sub = self.substrate.as_ref();
        for node_id in self.current_node_deployments.keys() {
            let node_client = self.nc_pool.get_node_client(sub, *node_id)?;
            let deployments = node_client.deployment_list()?;
            for deployment in deployments {
                let meta = resolve_metadata(sub, &deployment)?;
                let data = workloads::parse_deployment_data(&meta)?;

                for workload in deployment.workloads.iter() {
                    if workload.workload_type != zos::NETWORK_LIGHT_TYPE || workload.name != name {
                        continue;
                    }

                    let mut network =
                        workloads::new_network_light_from_workload(workload, *node_id)?;
                    network.solution_type = data.project_name;
                    z_nets.push(network);
                    node_deployments.insert(*node_id, deployment.contract_id);
                    break;
                }
            }
        }

        if z_nets.is_empty() {
            return Err(GridError::NotFound(format!(
                "failed to get network-light {name}"
            )));
        }

        let mut iter = z_nets.into_iter();
        if let Some(first) = iter.next() {
            znet = first;
        }

        for net in iter {
            znet.nodes.extend(net.nodes);
            znet.nodes_ip_range.extend(net.nodes_ip_range);
            znet.mycelium_keys.extend(net.mycelium_keys);
        }

        znet.node_deployment_id = node_deployments;
        self.networks
            .update_network_subnets(name, znet.nodes_ip_range.clone());
        Ok(znet)
    }

    pub fn load_k8s_from_grid(
        &mut self,
        node_ids: &[u32],
        deployment_name: &str,
    ) -> Result<workloads::K8sCluster, GridError> {
        let mut cluster_deployments = HashMap::new();
        let mut node_deployment_id = HashMap::new();

        for node_id in node_ids {
            let (_, deployment) = self.get_workload_in_deployment(*node_id, "", deployment_name)?;
            cluster_deployments.insert(*node_id, deployment.clone());
            node_deployment_id.insert(*node_id, deployment.contract_id);
        }

        let mut cluster = workloads::K8sCluster {
            master: None,
            workers: Vec::new(),
            token: String::new(),
            network_name: String::new(),
            flist: String::new(),
            flist_checksum: String::new(),
            entry_point: String::new(),
            solution_type: String::new(),
            ssh_key: String::new(),
            nodes_ip_range: HashMap::new(),
            node_deployment_id: node_deployment_id.clone(),
        };

        for (node_id, deployment) in cluster_deployments.iter() {
            let (workload_disk_size, workload_computed_ip, workload_computed_ip6) =
                workloads::compute_k8s_deployment_resources(deployment)?;

            for workload in &deployment.workloads {
                if workload.workload_type != zos::ZMACHINE_TYPE {
                    continue;
                }
                let disk_size = *workload_disk_size.get(&workload.name).unwrap_or(&0);
                let computed_ip = workload_computed_ip
                    .get(&workload.name)
                    .map_or("", String::as_str);
                let computed_ip6 = workload_computed_ip6
                    .get(&workload.name)
                    .map_or("", String::as_str);
                let node = workloads::new_k8s_node_from_workload(
                    workload,
                    *node_id,
                    disk_size,
                    computed_ip,
                    computed_ip6,
                )?;

                if workloads::is_master_node(workload)? {
                    let deployment_data = workloads::parse_deployment_data(&resolve_metadata(
                        self.substrate.as_ref(),
                        deployment,
                    )?)?;
                    cluster.master = Some(node);
                    cluster.solution_type = deployment_data.project_name;
                } else {
                    cluster.workers.push(node);
                }
            }
        }

        if cluster.master.is_none() {
            return Err(GridError::NotFound(format!(
                "failed to get master node for k8s cluster {deployment_name}"
            )));
        }

        let master = cluster
            .master
            .as_ref()
            .expect("master must exist at this point");
        cluster.network_name = master.vm.network_name.clone();
        cluster.node_deployment_id = node_deployment_id;
        cluster.ssh_key = master
            .vm
            .env_vars
            .get("SSH_KEY")
            .cloned()
            .unwrap_or_default();
        cluster.token = master
            .vm
            .env_vars
            .get("K3S_TOKEN")
            .cloned()
            .unwrap_or_default();
        cluster.flist = master.vm.flist.clone();
        cluster.flist_checksum = master.vm.flist_checksum.clone();
        cluster.entry_point = master.vm.entrypoint.clone();

        self.load_network_from_grid(&cluster.network_name)?;
        self.assign_nodes_ip_range(&mut cluster)?;

        Ok(cluster)
    }

    pub fn assign_nodes_ip_range(
        &self,
        cluster: &mut workloads::K8sCluster,
    ) -> Result<(), GridError> {
        let network = self.networks.get_network(&cluster.network_name);
        let mut nodes = HashMap::new();

        if let Some(master) = &cluster.master {
            let subnet = network.get_node_subnet(master.vm.node_id);
            if subnet.is_empty() {
                return Err(GridError::NotFound(format!(
                    "failed to get ip range for master node {}",
                    master.vm.node_id
                )));
            }
            nodes.insert(master.vm.node_id, subnet);
        }

        for worker in &cluster.workers {
            let subnet = network.get_node_subnet(worker.vm.node_id);
            if subnet.is_empty() {
                return Err(GridError::NotFound(format!(
                    "failed to get ip range for worker node {}",
                    worker.vm.node_id
                )));
            }
            nodes.insert(worker.vm.node_id, subnet);
        }

        cluster.nodes_ip_range = nodes;
        Ok(())
    }

    pub fn get_workload_in_deployment(
        &self,
        node_id: u32,
        workload_name: &str,
        deployment_name: &str,
    ) -> Result<(zos::Workload, zos::Deployment), GridError> {
        let Some(_) = self.current_node_deployments.get(&node_id) else {
            return Err(GridError::NotFound(format!("node {node_id} not indexed")));
        };

        let client = self
            .nc_pool
            .get_node_client(self.substrate.as_ref(), node_id)?;
        let deployments = client.deployment_list()?;
        for deployment in deployments {
            let meta = resolve_metadata(&*self.substrate, &deployment)?;
            let data = workloads::parse_deployment_data(&meta)?;
            if data.name != deployment_name {
                continue;
            }
            if workload_name.is_empty() {
                return Ok((zos::Workload::default(), deployment));
            }
            for wl in deployment.workloads.iter() {
                if wl.name == workload_name {
                    return Ok((wl.clone(), deployment));
                }
            }
            return Err(GridError::NotFound(format!(
                "workload {workload_name} not found in {deployment_name}"
            )));
        }
        Err(GridError::NotFound(format!(
            "deployment {deployment_name} not found on node {node_id}"
        )))
    }
}

fn resolve_metadata(
    substrate: &dyn subi::SubstrateExt,
    deployment: &zos::Deployment,
) -> Result<String, GridError> {
    if !deployment.metadata.trim().is_empty() {
        return Ok(deployment.metadata.clone());
    }

    if deployment.contract_id == 0 {
        return Err(GridError::NotFound("contract id not set".into()));
    }
    let contract = substrate.get_contract(deployment.contract_id)?;
    let data = contract.contract_type.node_contract.deployment_data.clone();
    if data.is_empty() {
        return Err(GridError::NotFound(format!(
            "contract {} has no deployment metadata",
            deployment.contract_id
        )));
    }
    Ok(data)
}

fn compute_wg_ip(ip_range: &str) -> Option<String> {
    let net = ip_range.parse::<ipnet::IpNet>().ok()?;
    match net.addr() {
        IpAddr::V4(ip) => Some(format!("100.64.{}.{}", ip.octets()[2], ip.octets()[3])),
        IpAddr::V6(_) => None,
    }
}

fn format_wg_config(
    wg_ip: &str,
    access_private_key: &str,
    node_public_key: &str,
    endpoint: &str,
    network_range: &str,
) -> String {
    format!(
        "\n[Interface]\nAddress = {wg_ip}\nPrivateKey = {access_private_key}\n[Peer]\nPublicKey = {node_public_key}\nAllowedIPs = {network_range}, 100.64.0.0/16\nPersistentKeepalive = 25\nEndpoint = {endpoint}\n\t",
        wg_ip = wg_ip,
        access_private_key = access_private_key,
        node_public_key = node_public_key,
        network_range = network_range,
        endpoint = endpoint,
    )
}

#[derive(Debug, Clone, Default)]
pub struct NetworkState {
    pub state: HashMap<String, Network>,
}

impl NetworkState {
    pub fn new() -> Self {
        Self {
            state: HashMap::new(),
        }
    }
    pub fn get_network(&self, name: &str) -> Network {
        self.state.get(name).cloned().unwrap_or_default()
    }
    pub fn update_network_subnets(&mut self, network_name: &str, ip_range: HashMap<u32, String>) {
        let network = self.get_network(network_name);
        let mut fresh = network;
        fresh.subnets = ip_range;
        self.state.insert(network_name.to_string(), fresh);
    }
    pub fn delete_network(&mut self, network_name: &str) {
        self.state.remove(network_name);
    }
}

#[derive(Debug, Clone, Default)]
pub struct Network {
    pub subnets: HashMap<u32, String>,
}
impl Network {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get_node_subnet(&self, node_id: u32) -> String {
        self.subnets.get(&node_id).cloned().unwrap_or_default()
    }
    pub fn set_node_subnet(&mut self, node_id: u32, subnet: &str) {
        self.subnets.insert(node_id, subnet.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{node::MockNodeClientGetter, subi, zos};
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

    fn gateway_name_workload() -> zos::Workload {
        zos::Workload {
            version: 0,
            name: "my.gw".to_string(),
            workload_type: zos::GATEWAY_NAME_PROXY_TYPE.to_string(),
            data: serde_json::json!({
                "name": "my.gw",
                "tls_passthrough": true,
                "backends": ["http://backend"],
            }),
            metadata: String::new(),
            description: "gateway".to_string(),
            result: zos::ResultData {
                created: 0,
                state: zos::STATE_OK.to_string(),
                error: String::new(),
                data: serde_json::json!({
                    "fqdn": "my.gw",
                }),
            },
        }
    }

    fn deployment_meta(solution_type: &str) -> String {
        serde_json::json!({
            "version": 3,
            "type": "Gateway Name",
            "name": "my.gw",
            "projectName": solution_type
        })
        .to_string()
    }

    #[test]
    fn load_gateway_name_from_grid_fills_contract_metadata_and_name_contract() {
        let node_id = 10;
        let contract_id = 100u64;

        let mut substrate = subi::MockSubstrate::new();
        substrate.add_node(subi::Node {
            id: node_id,
            certification: subi::Certification {
                is_certified: false,
            },
            resources: subi::NodeResources::default(),
        });
        substrate.add_contract(subi::Contract {
            contract_id,
            state: subi::ContractState::default(),
            contract_type: subi::ContractType {
                is_name_contract: false,
                is_node_contract: true,
                is_rent_contract: false,
                node_contract: subi::NodeContract {
                    node: node_id,
                    public_ips_count: 0,
                    deployment_data: String::new(),
                },
                rent_contract: subi::RentContract { node: 0 },
            },
        });
        substrate.set_contract_name("my.gw", 55);
        let substrate = std::sync::Arc::new(substrate);

        let nc_pool = MockNodeClientGetter::new();
        let wl = gateway_name_workload();
        let deployment = zos::Deployment {
            version: 0,
            twin_id: node_id,
            contract_id,
            metadata: deployment_meta("solution-a"),
            description: String::new(),
            expiration: 0,
            signature_requirement: zos::SignatureRequirement::default(),
            workloads: vec![wl],
        };
        nc_pool
            .insert_deployment(node_id, deployment)
            .expect("deployment insertion");

        let mut state = State::new(
            std::sync::Arc::new(nc_pool),
            substrate.clone() as std::sync::Arc<dyn subi::SubstrateExt + Send + Sync>,
        );
        state.store_contract_ids(node_id, &[contract_id]);

        let got = state
            .load_gateway_name_from_grid(node_id, "my.gw", "my.gw")
            .expect("load gateway");
        assert_eq!(got.node_id, node_id);
        assert_eq!(got.contract_id, contract_id);
        assert_eq!(got.name_contract_id, 55);
        assert_eq!(got.solution_type, "solution-a");
        assert_eq!(
            got.node_deployment_id.get(&node_id).copied(),
            Some(contract_id)
        );
    }

    #[test]
    fn load_gateway_name_from_grid_uses_deployment_name_for_workload_lookup() {
        let node_id = 11u32;
        let contract_id = 101u64;

        let mut substrate = subi::MockSubstrate::new();
        substrate.add_node(subi::Node {
            id: node_id,
            certification: subi::Certification {
                is_certified: false,
            },
            resources: subi::NodeResources::default(),
        });
        substrate.add_contract(subi::Contract {
            contract_id,
            state: subi::ContractState::default(),
            contract_type: subi::ContractType {
                is_name_contract: false,
                is_node_contract: true,
                is_rent_contract: false,
                node_contract: subi::NodeContract {
                    node: node_id,
                    public_ips_count: 0,
                    deployment_data: String::new(),
                },
                rent_contract: subi::RentContract { node: 0 },
            },
        });
        substrate.set_contract_name("gateway.example.com", 77);
        let substrate = std::sync::Arc::new(substrate);

        let nc_pool = MockNodeClientGetter::new();
        let workload = gateway_name_workload();
        let deployment = zos::Deployment {
            version: 0,
            twin_id: node_id,
            contract_id,
            metadata: serde_json::json!({
                "version": 3,
                "type": "Gateway Name",
                "name": "gateway-deployment",
                "projectName": "solution-b"
            })
            .to_string(),
            description: String::new(),
            expiration: 0,
            signature_requirement: zos::SignatureRequirement::default(),
            workloads: vec![zos::Workload {
                name: "gateway-deployment".to_string(),
                ..workload
            }],
        };
        nc_pool
            .insert_deployment(node_id, deployment)
            .expect("deployment insertion");

        let state = State::new(
            std::sync::Arc::new(nc_pool),
            substrate.clone() as std::sync::Arc<dyn subi::SubstrateExt + Send + Sync>,
        );
        let mut state = state;
        state.store_contract_ids(node_id, &[contract_id]);

        let got = state
            .load_gateway_name_from_grid(node_id, "gateway.example.com", "gateway-deployment")
            .expect("load gateway");
        assert_eq!(got.name_contract_id, 77);
        assert_eq!(got.contract_id, contract_id);
    }

    #[test]
    fn load_deployment_from_grid_applies_network_subnet_when_known() {
        let node_id = 11u32;
        let contract_id = 200u64;
        let flist_server = spawn_http_server(
            HashMap::from([("/vm.flist.md5".to_string(), "vm-checksum".to_string())]),
            1,
        );
        let node_contract = subi::Contract {
            contract_id,
            state: subi::ContractState::default(),
            contract_type: subi::ContractType {
                is_name_contract: false,
                is_node_contract: true,
                is_rent_contract: false,
                node_contract: subi::NodeContract {
                    node: node_id,
                    public_ips_count: 0,
                    deployment_data: serde_json::json!({
                        "version": 3,
                        "type": "vm",
                        "name": "dep",
                        "projectName": "vm-solution",
                    })
                    .to_string(),
                },
                rent_contract: subi::RentContract { node: 0 },
            },
        };
        let mut substrate = subi::MockSubstrate::new();
        substrate.add_node(subi::Node {
            id: node_id,
            certification: subi::Certification {
                is_certified: false,
            },
            resources: subi::NodeResources::default(),
        });
        substrate.add_contract(node_contract);
        let substrate = std::sync::Arc::new(substrate);

        let nc_pool = MockNodeClientGetter::new();
        let vm = workloads::VM {
            name: "vm".to_string(),
            node_id,
            network_name: "net".to_string(),
            description: "vm".to_string(),
            flist: format!("{flist_server}/vm.flist"),
            flist_checksum: String::new(),
            entrypoint: String::new(),
            public_ip: false,
            public_ip6: false,
            planetary: false,
            corex: false,
            ip: "10.0.0.2".to_string(),
            mycelium_ip_seed: Vec::new(),
            cpus: 1,
            memory_mb: 128,
            rootfs_size_mb: 0,
            mounts: Vec::new(),
            zlogs: Vec::new(),
            env_vars: std::collections::HashMap::new(),
            computed_ip: String::new(),
            computed_ip6: String::new(),
            planetary_ip: String::new(),
            mycelium_ip: String::new(),
            console_url: String::new(),
        };
        let dep = workloads::Deployment::new(
            "dep",
            node_id,
            "vm-solution",
            None,
            "net",
            Vec::new(),
            Vec::new(),
            vec![vm],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let metadata = dep.generate_metadata();
        let mut zdeploy = dep.zos_deployment(node_id).expect("zos deployment");
        zdeploy.metadata = metadata;
        zdeploy.contract_id = contract_id;

        // This test uses a generated deployment to keep workload layout deterministic.
        nc_pool
            .insert_deployment(node_id, zdeploy)
            .expect("insert dep");
        let network_workload = zos::Workload {
            version: 0,
            name: "net".to_string(),
            workload_type: zos::NETWORK_TYPE.to_string(),
            data: serde_json::json!({
                "ip_range": "10.10.0.0/16",
                "subnet": "10.10.10.0/24",
                "wireguard_private_key": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa=",
                "wireguard_listen_port": 1234,
            }),
            metadata: serde_json::json!({
                "version": 3,
                "user_accesses": [],
                "projectName": "solution-a",
            })
            .to_string(),
            description: String::new(),
            result: zos::ResultData::default(),
        };
        let network_deployment = zos::Deployment {
            version: 0,
            twin_id: node_id,
            contract_id: 201,
            metadata: serde_json::json!({
                "version": 3,
                "type": "network",
                "name": "network-dep",
                "projectName": "network-solution"
            })
            .to_string(),
            description: String::new(),
            expiration: 0,
            signature_requirement: zos::SignatureRequirement::default(),
            workloads: vec![network_workload],
        };
        nc_pool
            .insert_deployment(node_id, network_deployment)
            .expect("insert network dep");

        let mut state = State::new(
            std::sync::Arc::new(nc_pool),
            substrate.clone() as std::sync::Arc<dyn subi::SubstrateExt + Send + Sync>,
        );
        state.store_contract_ids(node_id, &[contract_id]);
        state.store_contract_ids(node_id, &[201]);
        state.networks.update_network_subnets(
            "net",
            vec![(node_id, "10.10.10.0/24".to_string())]
                .into_iter()
                .collect(),
        );

        let dep = state
            .load_deployment_from_grid(node_id, "dep")
            .expect("load dep");
        assert_eq!(dep.network_name, "net");
        assert_eq!(dep.ip_range, "10.10.10.0/24");
        assert_eq!(dep.solution_type, "vm-solution");
    }

    #[test]
    fn load_network_light_from_grid_merges_nodes_and_updates_state() {
        let node_a = 21u32;
        let node_b = 22u32;
        let contract_a = 301u64;
        let contract_b = 302u64;

        let mut substrate = subi::MockSubstrate::new();
        for node_id in [node_a, node_b] {
            substrate.add_node(subi::Node {
                id: node_id,
                certification: subi::Certification {
                    is_certified: false,
                },
                resources: subi::NodeResources::default(),
            });
        }
        let substrate = std::sync::Arc::new(substrate);

        let nc_pool = MockNodeClientGetter::new();
        for (node_id, contract_id, subnet, key) in [
            (node_a, contract_a, "10.20.1.0/24", "001122"),
            (node_b, contract_b, "10.20.2.0/24", "aabbcc"),
        ] {
            nc_pool
                .insert_deployment(
                    node_id,
                    zos::Deployment {
                        version: 0,
                        twin_id: node_id,
                        contract_id,
                        metadata: serde_json::json!({
                            "version": 3,
                            "type": "network-light",
                            "name": "nl-dep",
                            "projectName": "netlight-solution"
                        })
                        .to_string(),
                        description: String::new(),
                        expiration: 0,
                        signature_requirement: zos::SignatureRequirement::default(),
                        workloads: vec![zos::Workload {
                            version: 0,
                            name: "nl".to_string(),
                            workload_type: zos::NETWORK_LIGHT_TYPE.to_string(),
                            data: serde_json::json!({
                                "subnet": subnet,
                                "mycelium": {
                                    "hex_key": key,
                                }
                            }),
                            metadata: serde_json::json!({
                                "version": 4,
                                "user_accesses": [],
                            })
                            .to_string(),
                            description: String::new(),
                            result: zos::ResultData::default(),
                        }],
                    },
                )
                .expect("insert network-light deployment");
        }

        let mut state = State::new(
            std::sync::Arc::new(nc_pool),
            substrate.clone() as std::sync::Arc<dyn subi::SubstrateExt + Send + Sync>,
        );
        state.store_contract_ids(node_a, &[contract_a]);
        state.store_contract_ids(node_b, &[contract_b]);

        let znet = state
            .load_network_light_from_grid("nl")
            .expect("load network light");

        assert_eq!(znet.solution_type, "netlight-solution");
        let mut nodes = znet.nodes.clone();
        nodes.sort_unstable();
        assert_eq!(nodes, vec![node_a, node_b]);
        assert_eq!(
            znet.nodes_ip_range.get(&node_a).map(String::as_str),
            Some("10.20.1.0/24")
        );
        assert_eq!(
            znet.nodes_ip_range.get(&node_b).map(String::as_str),
            Some("10.20.2.0/24")
        );
        assert_eq!(
            znet.mycelium_keys.get(&node_a),
            Some(&vec![0x00, 0x11, 0x22])
        );
        assert_eq!(
            znet.mycelium_keys.get(&node_b),
            Some(&vec![0xaa, 0xbb, 0xcc])
        );
        assert_eq!(
            state.networks.get_network("nl").get_node_subnet(node_b),
            "10.20.2.0/24"
        );
    }

    #[test]
    fn load_k8s_from_grid_reconstructs_cluster_and_ip_ranges() {
        let master_node = 31u32;
        let worker_node = 32u32;
        let master_contract = 401u64;
        let worker_contract = 402u64;
        let network_contract = 403u64;
        let flist_server = spawn_http_server(
            HashMap::from([
                (
                    "/k3s-master.flist.md5".to_string(),
                    "master-checksum".to_string(),
                ),
                (
                    "/k3s-worker.flist.md5".to_string(),
                    "worker-checksum".to_string(),
                ),
            ]),
            2,
        );

        let mut substrate = subi::MockSubstrate::new();
        for node_id in [master_node, worker_node] {
            substrate.add_node(subi::Node {
                id: node_id,
                certification: subi::Certification {
                    is_certified: false,
                },
                resources: subi::NodeResources::default(),
            });
        }
        let substrate = std::sync::Arc::new(substrate);

        let nc_pool = MockNodeClientGetter::new();

        nc_pool
            .insert_deployment(
                master_node,
                zos::Deployment {
                    version: 0,
                    twin_id: master_node,
                    contract_id: master_contract,
                    metadata: serde_json::json!({
                        "version": 3,
                        "type": "kubernetes",
                        "name": "cluster-a",
                        "projectName": "k8s-solution"
                    })
                    .to_string(),
                    description: String::new(),
                    expiration: 0,
                    signature_requirement: zos::SignatureRequirement::default(),
                    workloads: vec![
                        zos::Workload {
                            version: 0,
                            name: "masterdisk".to_string(),
                            workload_type: zos::ZMOUNT_TYPE.to_string(),
                            data: serde_json::json!({
                                "size_gb": 50u64
                            }),
                            metadata: String::new(),
                            description: String::new(),
                            result: zos::ResultData::default(),
                        },
                        zos::Workload {
                            version: 0,
                            name: "masterip".to_string(),
                            workload_type: zos::PUBLIC_IP_TYPE.to_string(),
                            data: serde_json::json!({}),
                            metadata: String::new(),
                            description: String::new(),
                            result: zos::ResultData {
                                created: 0,
                                state: zos::STATE_OK.to_string(),
                                error: String::new(),
                                data: serde_json::json!({
                                    "ip": "203.0.113.10/32",
                                    "ipv6": "2001:db8::10/128"
                                }),
                            },
                        },
                        zos::Workload {
                            version: 0,
                            name: "master".to_string(),
                            workload_type: zos::ZMACHINE_TYPE.to_string(),
                            data: serde_json::json!({
                                "flist": format!("{flist_server}/k3s-master.flist"),
                                "network": {
                                    "planetary": true,
                                    "interfaces": [{
                                        "ip": "10.40.1.2",
                                        "network": "cluster-net"
                                    }]
                                },
                                "compute_cpu": 2u8,
                                "compute_memory_mb": 2048u64,
                                "rootfs_size_mb": 10240u64,
                                "env": {
                                    "SSH_KEY": "ssh-rsa AAAA",
                                    "K3S_TOKEN": "token123"
                                },
                                "entrypoint": "/sbin/zinit init"
                            }),
                            metadata: String::new(),
                            description: "master node".to_string(),
                            result: zos::ResultData {
                                created: 0,
                                state: zos::STATE_OK.to_string(),
                                error: String::new(),
                                data: serde_json::json!({
                                    "planetary_ip": "2001:db8::42",
                                    "mycelium_ip": "400::1",
                                    "console_url": "https://console.master"
                                }),
                            },
                        },
                    ],
                },
            )
            .expect("insert master deployment");

        nc_pool
            .insert_deployment(
                worker_node,
                zos::Deployment {
                    version: 0,
                    twin_id: worker_node,
                    contract_id: worker_contract,
                    metadata: serde_json::json!({
                        "version": 3,
                        "type": "kubernetes",
                        "name": "cluster-a",
                        "projectName": "k8s-solution"
                    })
                    .to_string(),
                    description: String::new(),
                    expiration: 0,
                    signature_requirement: zos::SignatureRequirement::default(),
                    workloads: vec![
                        zos::Workload {
                            version: 0,
                            name: "workerdisk".to_string(),
                            workload_type: zos::ZMOUNT_TYPE.to_string(),
                            data: serde_json::json!({
                                "size_gb": 60u64
                            }),
                            metadata: String::new(),
                            description: String::new(),
                            result: zos::ResultData::default(),
                        },
                        zos::Workload {
                            version: 0,
                            name: "worker".to_string(),
                            workload_type: zos::ZMACHINE_TYPE.to_string(),
                            data: serde_json::json!({
                                "flist": format!("{flist_server}/k3s-worker.flist"),
                                "network": {
                                    "interfaces": [{
                                        "ip": "10.40.2.2",
                                        "network": "cluster-net"
                                    }]
                                },
                                "compute_cpu": 2u8,
                                "compute_memory_mb": 1024u64,
                                "rootfs_size_mb": 10240u64,
                                "env": {
                                    "K3S_URL": "https://10.40.1.2:6443",
                                    "K3S_TOKEN": "token123"
                                },
                                "entrypoint": "/sbin/zinit init"
                            }),
                            metadata: String::new(),
                            description: "worker node".to_string(),
                            result: zos::ResultData {
                                created: 0,
                                state: zos::STATE_OK.to_string(),
                                error: String::new(),
                                data: serde_json::json!({
                                    "console_url": "https://console.worker"
                                }),
                            },
                        },
                    ],
                },
            )
            .expect("insert worker deployment");

        for (node_id, subnet) in [(master_node, "10.40.1.0/24"), (worker_node, "10.40.2.0/24")] {
            nc_pool
                .insert_deployment(
                    node_id,
                    zos::Deployment {
                        version: 0,
                        twin_id: node_id,
                        contract_id: network_contract + u64::from(node_id - master_node),
                        metadata: serde_json::json!({
                            "version": 3,
                            "type": "network",
                            "name": "cluster-net-dep",
                            "projectName": "network-solution"
                        })
                        .to_string(),
                        description: String::new(),
                        expiration: 0,
                        signature_requirement: zos::SignatureRequirement::default(),
                        workloads: vec![zos::Workload {
                            version: 0,
                            name: "cluster-net".to_string(),
                            workload_type: zos::NETWORK_TYPE.to_string(),
                            data: serde_json::json!({
                                "ip_range": "10.40.0.0/16",
                                "subnet": subnet,
                                "wireguard_private_key": "",
                                "wireguard_listen_port": 0
                            }),
                            metadata: serde_json::json!({
                                "version": 3,
                                "user_accesses": [],
                            })
                            .to_string(),
                            description: String::new(),
                            result: zos::ResultData::default(),
                        }],
                    },
                )
                .expect("insert network deployment");
        }

        let mut state = State::new(
            std::sync::Arc::new(nc_pool),
            substrate.clone() as std::sync::Arc<dyn subi::SubstrateExt + Send + Sync>,
        );
        state.store_contract_ids(master_node, &[master_contract, network_contract]);
        state.store_contract_ids(worker_node, &[worker_contract, network_contract + 1]);

        let cluster = state
            .load_k8s_from_grid(&[master_node, worker_node], "cluster-a")
            .expect("load k8s cluster");

        let master = cluster.master.expect("master node");
        assert_eq!(master.vm.name, "master");
        assert_eq!(master.disk_size_gb, 50);
        assert_eq!(master.vm.flist_checksum, "master-checksum");
        assert_eq!(master.vm.computed_ip, "203.0.113.10");
        assert_eq!(master.vm.computed_ip6, "2001:db8::10");
        assert_eq!(cluster.workers.len(), 1);
        assert_eq!(cluster.workers[0].vm.name, "worker");
        assert_eq!(cluster.workers[0].disk_size_gb, 60);
        assert_eq!(cluster.workers[0].vm.flist_checksum, "worker-checksum");
        assert_eq!(cluster.network_name, "cluster-net");
        assert_eq!(cluster.solution_type, "k8s-solution");
        assert_eq!(cluster.ssh_key, "ssh-rsa AAAA");
        assert_eq!(cluster.token, "token123");
        assert_eq!(
            cluster.nodes_ip_range.get(&master_node).map(String::as_str),
            Some("10.40.1.0/24")
        );
        assert_eq!(
            cluster.nodes_ip_range.get(&worker_node).map(String::as_str),
            Some("10.40.2.0/24")
        );
    }
}
