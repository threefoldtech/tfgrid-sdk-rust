# `sdk-grid-rust`

Rust reimplementation of the ThreeFold `tfgrid-sdk-go` `grid-client`.

The crate currently has two layers:

- Pure Rust reconstruction/state helpers and deployer mocks for fast local tests.
- A `GridClient` that can create TFChain contracts and deploy workloads over RMB.

## Current `GridClient` Scope

The client implementation in [mod.rs](/home/xmonader/wspace/geomind/sdk-grid-rust/src/grid_client/mod.rs) supports:

- Devnet twin relay setup
- Node contract creation on TFChain
- RMB JWT signing and websocket transport
- RMB encrypted response handling
- `network-light` deployment
- `vm-light` deployment on the same node

The successful Rust-only deployment path is exercised by [examples/deploy_small_vm.rs](/home/xmonader/wspace/geomind/sdk-grid-rust/examples/deploy_small_vm.rs).

## GridClient API

The main API is centered around these public types:

- `GridClient`
- `GridClientConfig`
- `VmLightDeployment`
- `VmLightSpec`
- `VmDeployment`
- `VmSpec`
- `NetworkTarget`
- `NetworkLightSpec`
- `FullNetworkTarget`
- `FullNetworkSpec`
- `ExistingNetworkSpec`
- `NodePlacement`
- `NodeRequirements`
- `VolumeMountSpec`

`deploy_small_vm()` still exists as a convenience wrapper, but the main configurable entry point is:

```rust
client.deploy_vm_light(request).await?;
```

Typical builder-style usage:

```rust
let request = VmLightDeployment::builder()
    .auto_with(
        NodeRequirements::builder()
            .min_cru(2)
            .min_memory_bytes(2 * 1024 * 1024 * 1024)
            .min_rootfs_bytes(20 * 1024 * 1024 * 1024)
            .build(),
    )
    .create_network(NetworkLightSpec::builder().name("demo-net").build())
    .vm(
        VmLightSpec::builder()
            .name("demo-vm")
            .cpu(2)
            .memory_bytes(2 * 1024 * 1024 * 1024)
            .env("SSH_KEY", "ssh-ed25519 ...")
            .build(),
    )
    .build();
```

Named network presets are built in:

```rust
use tfgrid_sdk_rust::{GridClient, GridClientConfig};

let dev = GridClientConfig::devnet();
let qa = GridClientConfig::qanet();
let test = GridClientConfig::testnet();
let main = GridClientConfig::mainnet();

let client = GridClient::qanet(&mnemonic).await?;
```

You can also resolve a preset dynamically:

```rust
use tfgrid_sdk_rust::{GridClient, GridClientConfig};

let config = GridClientConfig::from_network("mainnet")?;

let client = GridClient::new(&mnemonic, config).await?;
```

Or use the builder with a named preset and explicit overrides:

```rust
use std::time::Duration;
use tfgrid_sdk_rust::{GridClient, GridClientConfig, MAIN_NETWORK};

let config = GridClientConfig::builder()
    .network(MAIN_NETWORK)
    .substrate_urls(vec![
        "wss://tfchain.us.grid.tf/ws".to_string(),
        "wss://tfchain.grid.tf/ws".to_string(),
    ])
    .http_timeout(Duration::from_secs(30))
    .rmb_timeout(Duration::from_secs(30))
    .build();

let client = GridClient::new(&mnemonic, config).await?;
```

With that request you can now control:

- CPU
- Memory
- Root filesystem size
- Flist
- Entrypoint
- Environment variables
- Mounts
- GPU list
- CoreX toggle
- Automatic placement requirements
- Fixed node placement
- New or existing `network-light` usage

For full `zmachine` deployments, use:

```rust
client.deploy_vm(request).await?;
```

Lifecycle helpers:

- `cancel_contract(contract_id)`
- `cancel_deployment_outcome(&outcome)`

## Examples

Set your mnemonic in the environment first:

```bash
export MNEMONIC='your devnet mnemonic here'
```

Select a preset network when you do not want the default devnet:

```bash
export GRID_NETWORK=mainnet
```

Deploy a small VM on the selected preset network:

```bash
cargo run --example deploy_small_vm
```

Deploy a configurable VM on the selected preset network:

```bash
cargo run --example deploy_custom_vm
```

Deploy a `vm-light` with an attached volume:

```bash
cargo run --example deploy_vm_with_volume
```

Deploy a full `zmachine` with public IPv4:

```bash
cargo run --example deploy_public_vm
```

Cancel a live deployment outcome:

```bash
export NODE_TWIN_ID=<node twin id>
export VM_CONTRACT_ID=<vm contract id>
export NETWORK_CONTRACT_ID=<network contract id>
cargo run --example cancel_deployment
```

Print the RMB token for debugging:

```bash
cargo run --example print_rmb_token
```

Deploy a VM on an existing `network-light` contract:

```bash
export NODE_ID=327
export NODE_TWIN_ID=11394
export NETWORK_NAME=rust_net_light_123
export VM_IP=10.50.2.5
cargo run --example deploy_vm_on_existing_network
```

Enable client tracing when debugging relay or workload behavior:

```bash
TFGRID_DEBUG=1 cargo run --example deploy_small_vm
```

## Verification

```bash
cargo test
```

## Notes

- `GridClient::devnet()`, `GridClient::qanet()`, `GridClient::testnet()`, and `GridClient::mainnet()` use built-in endpoint presets.
- `GridClientConfig::from_network(...)` accepts `dev`, `devnet`, `qa`, `qanet`, `test`, `testnet`, `main`, and `mainnet`.
- `GridClient::new()` lets you override substrate, gridproxy, graphql, relay, KYC, sentry, and timeout settings on top of those presets.
- The example code will try to load a public SSH key from `SSH_KEY_PATH` or common files under `~/.ssh/`.
- Earlier broken live attempts can leave stray devnet contracts behind. Use the cancellation helpers or clean them up manually.
