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

## Examples

Set your mnemonic in the environment first:

```bash
export MNEMONIC='your devnet mnemonic here'
```

Deploy a small VM on devnet:

```bash
cargo run --example deploy_small_vm
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
- Earlier broken live attempts can leave stray devnet contracts behind. Clean those up manually or through a future cancellation helper.
