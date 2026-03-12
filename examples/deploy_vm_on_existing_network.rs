use std::{env, fs, path::PathBuf};

use tfgrid_sdk_rust::{LiveClient, VmLightDeployment, VmLightSpec};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mnemonic = env::var("MNEMONIC").map_err(|_| "MNEMONIC is required")?;
    let node_id: u32 = env::var("NODE_ID")
        .map_err(|_| "NODE_ID is required")?
        .parse()?;
    let node_twin_id: u32 = env::var("NODE_TWIN_ID")
        .map_err(|_| "NODE_TWIN_ID is required")?
        .parse()?;
    let network_name = env::var("NETWORK_NAME").map_err(|_| "NETWORK_NAME is required")?;
    let vm_ip = env::var("VM_IP").map_err(|_| "VM_IP is required")?;
    let ssh_key = load_ssh_key().ok();
    let client = LiveClient::devnet(&mnemonic).await?;
    let request = VmLightDeployment::builder()
        .fixed_node(node_id, node_twin_id)
        .existing_network(network_name, vm_ip)
        .vm({
            let mut vm = VmLightSpec::builder();
            if let Some(key) = ssh_key.as_deref().filter(|value| !value.trim().is_empty()) {
                vm = vm.env("SSH_KEY", key.trim());
            }
            vm.build()
        })
        .build();
    let outcome = client.deploy_vm_light(request).await?;
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
