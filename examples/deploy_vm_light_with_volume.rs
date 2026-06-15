use std::{collections::HashMap, env, fs, path::PathBuf};

use tfgrid_sdk_rust::{
    DEV_NETWORK, GridClient, GridClientConfig, NetworkLightSpec, NodeRequirements,
    VmLightDeployment, VmLightSpec, VolumeMountSpec,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mnemonic = env::var("MNEMONIC").map_err(|_| "MNEMONIC is required")?;
    let network = env::var("GRID_NETWORK").unwrap_or_else(|_| DEV_NETWORK.to_string());
    let ssh_key = load_ssh_key().ok();

    let mut env_vars = HashMap::new();
    if let Some(key) = ssh_key.as_deref().filter(|value| !value.trim().is_empty()) {
        env_vars.insert("SSH_KEY".to_string(), key.trim().to_string());
    }

    let request = VmLightDeployment::builder()
        .auto_with(NodeRequirements::default())
        .create_network(NetworkLightSpec::builder().build())
        .vm({
            let mut vm = VmLightSpec::builder().volume(
                VolumeMountSpec::new(
                    env::var("VOLUME_NAME").unwrap_or_else(|_| "data".to_string()),
                    env::var("VOLUME_BYTES")
                        .ok()
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(5 * 1024 * 1024 * 1024),
                    env::var("MOUNTPOINT").unwrap_or_else(|_| "/data".to_string()),
                )
                .description("extra data volume"),
            );
            for (key, value) in env_vars {
                vm = vm.env(key, value);
            }
            vm.build()
        })
        .build();

    let client = GridClient::new(&mnemonic, GridClientConfig::from_network(&network)?).await?;
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
