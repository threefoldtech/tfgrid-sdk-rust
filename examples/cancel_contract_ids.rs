use std::env;

use tfgrid_sdk_rust::{DEV_NETWORK, GridClient, GridClientConfig};

/// Cancel one or more contracts by id. Useful for cleaning up orphaned
/// network contracts left behind when a deployment fails mid-flight.
///
/// Usage: CONTRACT_IDS="2098271,2098272" cargo run --example cancel_contract_ids
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mnemonic = env::var("MNEMONIC").map_err(|_| "MNEMONIC is required")?;
    let network = env::var("GRID_NETWORK").unwrap_or_else(|_| DEV_NETWORK.to_string());
    let ids: Vec<u64> = env::var("CONTRACT_IDS")
        .map_err(|_| "CONTRACT_IDS is required (comma-separated)")?
        .split(',')
        .filter_map(|value| value.trim().parse().ok())
        .collect();

    let client = GridClient::new(&mnemonic, GridClientConfig::from_network(&network)?).await?;
    for id in ids {
        match client.cancel_contract(id).await {
            Ok(()) => println!("cancelled contract {id}"),
            Err(err) => println!("failed to cancel contract {id}: {err}"),
        }
    }
    Ok(())
}
