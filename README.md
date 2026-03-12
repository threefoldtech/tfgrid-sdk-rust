# `sdk-grid-rust`

Rust reimplementation of the ThreeFold `tfgrid-sdk-go` `grid-client`.

The crate has two layers:

- Pure Rust reconstruction/state helpers and deployer mocks for fast local tests.
- A `GridClient` that can create TFChain contracts and deploy workloads over RMB.

## Scope

The `GridClient` implementation in [mod.rs](/home/xmonader/wspace/geomind/sdk-grid-rust/src/grid_client/mod.rs) currently supports:

- TFChain twin lookup and relay setup
- Node contract creation on TFChain
- RMB JWT signing and websocket transport
- RMB encrypted response handling
- `network-light` deployment
- `vm-light` deployment
- full `zmachine` deployment
- attached volume creation for VM workloads
- contract cancellation helpers

The Rust-only deployment path is exercised by [deploy_small_vm.rs](/home/xmonader/wspace/geomind/sdk-grid-rust/examples/deploy_small_vm.rs), [deploy_custom_vm.rs](/home/xmonader/wspace/geomind/sdk-grid-rust/examples/deploy_custom_vm.rs), and [deploy_public_vm.rs](/home/xmonader/wspace/geomind/sdk-grid-rust/examples/deploy_public_vm.rs).

## Quick Start

Set a mnemonic and optionally a target preset network:

```bash
export MNEMONIC='your mnemonic here'
export GRID_NETWORK=dev
```

Run one of the live examples:

```bash
cargo run --example deploy_small_vm
```

Supported `GRID_NETWORK` values:

- `dev` / `devnet`
- `qa` / `qanet`
- `test` / `testnet`
- `main` / `mainnet`

If `GRID_NETWORK` is not set, the examples default to `dev`.

## Public API

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

Convenience entry points:

```rust
let client = GridClient::devnet(&mnemonic).await?;
let client = GridClient::qanet(&mnemonic).await?;
let client = GridClient::testnet(&mnemonic).await?;
let client = GridClient::mainnet(&mnemonic).await?;
```

Main deployment entry points:

```rust
client.deploy_vm_light(request).await?;
client.deploy_vm(request).await?;
```

`deploy_small_vm()` still exists as a convenience wrapper for a minimal `vm-light`.

Typical builder-style `vm-light` usage:

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

## Network Presets

Built-in config presets:

```rust
use tfgrid_sdk_rust::{GridClient, GridClientConfig};

let dev = GridClientConfig::devnet();
let qa = GridClientConfig::qanet();
let test = GridClientConfig::testnet();
let main = GridClientConfig::mainnet();

let client = GridClient::qanet(&mnemonic).await?;
```

Dynamic preset selection:

```rust
use tfgrid_sdk_rust::{GridClient, GridClientConfig};

let config = GridClientConfig::from_network("mainnet")?;

let client = GridClient::new(&mnemonic, config).await?;
```

Builder-based preset selection with overrides:

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

Preset configs include:

- substrate websocket URLs
- gridproxy URLs
- graphql URLs
- relay URLs
- KYC URL
- sentry DSN
- HTTP timeout
- RMB timeout

Override behavior:

- `substrate_url(...)`, `grid_proxy_url(...)`, `graphql_url(...)`, and `relay_url(...)` pin the primary endpoint explicitly.
- `substrate_urls(...)`, `grid_proxy_urls(...)`, `graphql_urls(...)`, and `relay_urls(...)` replace the candidate list and make the first supplied URL the primary endpoint unless you later override the singular field explicitly.
- URLs are normalized before use, so trailing `/` is stripped from configured endpoints.

## Workload Configuration

For `VmLightDeployment`, the public builders let you control:

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

For `VmDeployment`, you can additionally request full networking features such as public IPv4, public IPv6, and planetary networking.

Lifecycle helpers:

- `cancel_contract(contract_id)`
- `cancel_deployment_outcome(&outcome)`

## Examples

Deploy a small VM on the selected preset network:

```bash
cargo run --example deploy_small_vm
```

Deploy a configurable VM on the selected preset network:

```bash
cargo run --example deploy_custom_vm
```

Deploy a VM on an existing `network-light` contract:

```bash
export NODE_ID=327
export NODE_TWIN_ID=11394
export NETWORK_NAME=rust_net_light_123
export VM_IP=10.50.2.5
cargo run --example deploy_vm_on_existing_network
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

Enable client tracing when debugging relay or workload behavior:

```bash
TFGRID_DEBUG=1 cargo run --example deploy_small_vm
```

## Verification

```bash
cargo test
cargo check --examples
```

## Notes

- `GridClient::devnet()`, `GridClient::qanet()`, `GridClient::testnet()`, and `GridClient::mainnet()` use built-in endpoint presets.
- `GridClientConfig::from_network(...)` accepts `dev`, `devnet`, `qa`, `qanet`, `test`, `testnet`, `main`, and `mainnet`.
- `GridClient::new()` lets you override substrate, gridproxy, graphql, relay, KYC, sentry, and timeout settings on top of those presets.
- The example code will try to load a public SSH key from `SSH_KEY_PATH` or common files under `~/.ssh/`.
- Live deployment has been verified on devnet in this repository. Other named presets are first-class in the API, but have not all been smoke-tested here yet.
- Earlier broken live attempts can leave stray contracts behind. Use the cancellation helpers or clean them up manually.
