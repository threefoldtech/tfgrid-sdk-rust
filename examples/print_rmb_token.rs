use std::env;

use tfgrid_sdk_rust::{DEV_NETWORK, GridClient, GridClientConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mnemonic = env::var("MNEMONIC").map_err(|_| "MNEMONIC is required")?;
    let network = env::var("GRID_NETWORK").unwrap_or_else(|_| DEV_NETWORK.to_string());
    let client = GridClient::new(&mnemonic, GridClientConfig::from_network(&network)?).await?;
    println!("twin: {}", client.twin_id());
    println!("token: {}", client.debug_rmb_token()?);
    Ok(())
}
