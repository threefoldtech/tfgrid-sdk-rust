use std::{collections::HashMap, env, fs, path::PathBuf};

use tfgrid_sdk_rust::{
    LiveClient, NetworkLightSpec, NetworkTarget, NodePlacement, NodeRequirements,
    VmLightDeployment, VmLightSpec, VolumeMountSpec,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mnemonic = env::var("MNEMONIC").map_err(|_| "MNEMONIC is required")?;
    let ssh_key = load_ssh_key().ok();

    let mut env_vars = HashMap::new();
    if let Some(key) = ssh_key.as_deref().filter(|value| !value.trim().is_empty()) {
        env_vars.insert("SSH_KEY".to_string(), key.trim().to_string());
    }

    let request = VmLightDeployment::builder()
        .placement(NodePlacement::Auto(NodeRequirements::default()))
        .network(NetworkTarget::Create(NetworkLightSpec::default()))
        .vm(VmLightSpec {
            env: env_vars,
            volumes: vec![VolumeMountSpec {
                name: env::var("VOLUME_NAME").unwrap_or_else(|_| "data".to_string()),
                size_bytes: env::var("VOLUME_BYTES")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(5 * 1024 * 1024 * 1024),
                mountpoint: env::var("MOUNTPOINT").unwrap_or_else(|_| "/data".to_string()),
                description: "extra data volume".to_string(),
            }],
            ..Default::default()
        })
        .build();

    let client = LiveClient::devnet(&mnemonic).await?;
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
