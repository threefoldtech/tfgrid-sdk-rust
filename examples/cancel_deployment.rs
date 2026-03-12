use std::env;

use tfgrid_sdk_rust::{DEV_NETWORK, DeploymentOutcome, GridClient, GridClientConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mnemonic = env::var("MNEMONIC").map_err(|_| "MNEMONIC is required")?;
    let network = env::var("GRID_NETWORK").unwrap_or_else(|_| DEV_NETWORK.to_string());
    let node_twin_id: u32 = env::var("NODE_TWIN_ID")
        .map_err(|_| "NODE_TWIN_ID is required")?
        .parse()?;
    let vm_contract_id: u64 = env::var("VM_CONTRACT_ID")
        .map_err(|_| "VM_CONTRACT_ID is required")?
        .parse()?;
    let network_contract_id: u64 = env::var("NETWORK_CONTRACT_ID")
        .map_err(|_| "NETWORK_CONTRACT_ID is required")?
        .parse()?;

    let client = GridClient::new(&mnemonic, GridClientConfig::from_network(&network)?).await?;
    let outcome = DeploymentOutcome {
        node_id: 0,
        node_twin_id,
        network_name: String::new(),
        network_contract_id,
        vm_name: String::new(),
        vm_contract_id,
        vm_ip: String::new(),
        mycelium_ip: String::new(),
        public_ipv4: String::new(),
        public_ipv6: String::new(),
        console_url: String::new(),
    };
    client.cancel_deployment_outcome(&outcome).await?;
    println!(
        "cancelled vm contract {} and network contract {}",
        vm_contract_id, network_contract_id
    );
    Ok(())
}
