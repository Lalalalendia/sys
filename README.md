# SYS

Resilient connectivity / Personal Exit research and implementation workspace.

## Current slice

Rust Personal Exit spike using Iroh as a candidate connectivity substrate.

Scope:
- exact packet wire v0;
- evolvable control envelope;
- product-session state above transport connections;
- Iroh-backed connectivity adapter boundary;
- relay-only lab client/exit binaries.

Deliberately out of scope for the first slice: OS TUN capture, nftables/NAT, GUI, cloud account service, public discovery, custom censorship transports, and durable resume across process restart.

Iroh is pinned to commit `d4490fcfde8e4dc7260a44a2a982c654748595e2`, after the bounded pending-path retry fix. Production TUN integration still requires a direct-address candidate filter / underlay socket-protection solution before full-tunnel routing is enabled.

## M1 lab intent

1. Run a local relay in dev mode from the pinned Iroh checkout:
   `cargo run -p iroh-relay --features server -- --dev`
2. Run `pe-exit-lab [relay-url]` and copy the printed exit EndpointId.
3. Run `pe-client-lab <exit-endpoint-id> [relay-url]`.
4. M1 passes only after the control hello and packet-datagram echo both complete.

## Validation status

The initial source was generated in an environment without Cargo/Rust. It is not yet a claimed green build. First gate in a Rust-capable runner:

```text
cargo check --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```
