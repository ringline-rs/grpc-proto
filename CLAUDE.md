# CLAUDE.md

## Build & Test Commands

```bash
# Build
cargo build
cargo build --all-features

# Lint
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --all-features -- -D warnings

# Test
cargo test --all
cargo test --all --all-features
cargo test --all --release

# Docs
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
```

## Architecture

gRPC protocol framing layer built on top of http2-proto. No runtime dependencies.

### Features

- `tls` (default) — enables TLS re-exports from http2-proto

### Key Types

- `Channel` — gRPC client channel wrapping an HTTP/2 connection
- `CallBuilder` / `Call` / `CallEvent` — client-side RPC abstractions
- `Server` — gRPC server wrapping an HTTP/2 server connection
- `Request` / `GrpcServerEvent` — server-side RPC abstractions
- `MessageDecoder` — stateful gRPC message frame decoder
- `Status` / `Code` — gRPC status codes
- `Metadata` — gRPC metadata (headers/trailers)
