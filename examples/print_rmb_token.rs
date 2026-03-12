use std::env;

use tfgrid_sdk_rust::GridClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mnemonic = env::var("MNEMONIC").map_err(|_| "MNEMONIC is required")?;
    let client = GridClient::devnet(&mnemonic).await?;
    println!("twin: {}", client.twin_id());
    println!("token: {}", client.debug_rmb_token()?);
    Ok(())
}
