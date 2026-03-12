use std::{collections::HashMap, env, fs, path::PathBuf};

use tfgrid_sdk_rust::{
    LiveClient, NetworkLightSpec, NodeRequirements, VmLightDeployment, VmLightSpec,
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
        .auto_with(
            NodeRequirements::builder()
                .min_cru(2)
                .min_memory_bytes(2 * 1024 * 1024 * 1024)
                .min_rootfs_bytes(20 * 1024 * 1024 * 1024)
                .build(),
        )
        .create_network(NetworkLightSpec::builder().build())
        .vm({
            let mut vm = VmLightSpec::builder()
                .cpu(
                    env::var("CPU")
                        .ok()
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(2),
                )
                .memory_bytes(
                    env::var("MEMORY_BYTES")
                        .ok()
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(2 * 1024 * 1024 * 1024),
                )
                .rootfs_size_bytes(
                    env::var("ROOTFS_BYTES")
                        .ok()
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(20 * 1024 * 1024 * 1024),
                )
                .flist(env::var("FLIST").unwrap_or_else(|_| {
                    "https://hub.grid.tf/tf-official-apps/base:latest.flist".to_string()
                }));
            if let Ok(name) = env::var("VM_NAME") {
                vm = vm.name(name);
            }
            for (key, value) in env_vars {
                vm = vm.env(key, value);
            }
            vm.build()
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
