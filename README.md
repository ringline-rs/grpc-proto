# grpc-proto

gRPC protocol framing layer built on top of [http2-proto](https://crates.io/crates/http2-proto).

A standalone gRPC implementation with no async runtime dependencies.

## Features

- gRPC message framing (length-prefixed)
- Unary and streaming RPC support (client and server)
- gRPC status codes and metadata
- Timeout support
- Optional **TLS** transport via the `tls` feature flag (enabled by default)

## Usage

```toml
[dependencies]
grpc-proto = "0.0.1"

# Without TLS:
grpc-proto = { version = "0.0.1", default-features = false }
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
