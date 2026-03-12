# `sdk-grid-rust`

Rust reimplementation of the ThreeFold `tfgrid-sdk-go` `grid-client`.

The crate currently has two layers:

- Pure Rust reconstruction/state helpers and deployer mocks for fast local tests.
- A live devnet client that can create TFChain contracts and deploy `network-light` and `vm-light` workloads over RMB.

## Current live scope

The live client in [src/live.rs](/home/xmonader/wspace/geomind/sdk-grid-rust/src/live.rs) supports:

- Devnet twin relay setup
- Node contract creation on TFChain
- RMB JWT signing and websocket transport
- RMB encrypted response handling
- `network-light` deployment
- `vm-light` deployment on the same node

The successful Rust-only deployment path is exercised by [examples/deploy_small_vm.rs](/home/xmonader/wspace/geomind/sdk-grid-rust/examples/deploy_small_vm.rs).

## Live API

The live API is centered around these public types:

- `LiveClient`
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

`deploy_small_vm()` still exists as a convenience wrapper, but the configurable entry point is:

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

Deploy a small VM on devnet:

```bash
cargo run --example deploy_small_vm
```

Deploy a configurable VM on devnet:

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

Enable live-path tracing when debugging relay or workload behavior:

```bash
TFGRID_DEBUG=1 cargo run --example deploy_small_vm
```

## Verification

```bash
cargo test
```

## Notes

- The live client is intentionally devnet-focused today.
- The example code will try to load a public SSH key from `SSH_KEY_PATH` or common files under `~/.ssh/`.
- Earlier broken live attempts can leave stray devnet contracts behind. Use the cancellation helpers or clean them up manually.
