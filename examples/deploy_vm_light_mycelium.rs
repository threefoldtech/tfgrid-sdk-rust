use std::{
    collections::HashMap,
    env, fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use tfgrid_sdk_rust::{
    DEV_NETWORK, GridClient, GridClientConfig, NetworkLightSpec, NodeRequirements, VmLightDeployment,
    VmLightSpec, zos,
};

/// Deploy a `vm-light` reachable over a Mycelium IP.
///
/// A `network-light` always carries a Mycelium key and a `vm-light` always
/// attaches a Mycelium interface, so the Mycelium IP is created automatically.
/// Here we pin an explicit key/seed so the address is reproducible.
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
        .auto_with(
            NodeRequirements::builder()
                .min_cru(2)
                .min_memory_bytes(2 * 1024 * 1024 * 1024)
                .min_rootfs_bytes(20 * 1024 * 1024 * 1024)
                .build(),
        )
        .create_network(
            NetworkLightSpec::builder()
                .mycelium_key(varying_bytes(zos::MYCELIUM_KEY_LEN))
                .build(),
        )
        .vm({
            let mut vm = VmLightSpec::builder()
                .cpu(2)
                .memory_bytes(2 * 1024 * 1024 * 1024)
                .rootfs_size_bytes(20 * 1024 * 1024 * 1024)
                .mycelium_seed(varying_bytes(zos::MYCELIUM_IP_SEED_LEN));
            for (key, value) in env_vars {
                vm = vm.env(key, value);
            }
            vm.build()
        })
        .build();

    let client = GridClient::new(&mnemonic, GridClientConfig::from_network(&network)?).await?;
    let outcome = client.deploy_vm_light(request).await?;
    println!("{}", serde_json::to_string_pretty(&outcome)?);
    println!("mycelium ip: {}", outcome.mycelium_ip);
    Ok(())
}

/// Length-correct, time-varying bytes so repeated runs get distinct addresses
/// without pulling in an RNG dependency.
fn varying_bytes(len: usize) -> Vec<u8> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    (0..len)
        .map(|i| (nanos >> (i % 16 * 8)) as u8 ^ (i as u8).wrapping_mul(31))
        .collect()
}

fn load_ssh_key() -> Result<String, std::io::Error> {
    if let Ok(path) = env::var("SSH_KEY_PATH") {
        return fs::read_to_string(path);
    }

    let home = env::var("HOME").unwrap_or_default();
    let candidates = [".ssh/id_ed25519.pub", ".ssh/id_rsa.pub", ".ssh/id_ecdsa.pub"];
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
