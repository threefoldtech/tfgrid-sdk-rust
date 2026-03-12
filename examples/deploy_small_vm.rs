use std::{env, fs, path::PathBuf};

use tfgrid_sdk_rust::{DEV_NETWORK, GridClient, GridClientConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mnemonic = env::var("MNEMONIC").map_err(|_| "MNEMONIC is required")?;
    let network = env::var("GRID_NETWORK").unwrap_or_else(|_| DEV_NETWORK.to_string());
    let ssh_key = load_ssh_key().ok();
    let client = GridClient::new(&mnemonic, GridClientConfig::from_network(&network)?).await?;
    let outcome = client.deploy_small_vm(ssh_key.as_deref()).await?;
    println!("{}", serde_json::to_string_pretty(&outcome)?);
    Ok(())
}

fn load_ssh_key() -> Result<String, std::io::Error> {
    if let Ok(path) = env::var("SSH_KEY_PATH") {
        return fs::read_to_string(path);
    }

    let home = env::var("HOME").unwrap_or_default();
    let candidates = [
        ".ssh/id_ed25519.pub",
        ".ssh/id_rsa.pub",
        ".ssh/id_ecdsa.pub",
    ];
    for candidate in candidates {
        let path = PathBuf::from(&home).join(candidate);
        if path.exists() {
            return fs::read_to_string(path);
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "no ssh public key found",
    ))
}
