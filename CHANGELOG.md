# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.2] - 2026-08-19

### Fixed

- `encode_message` and `encode_message_with_compression` reject a message
  longer than `u32::MAX` instead of silently truncating the length prefix. The
  gRPC prefix is a `u32`, so `data.len() as u32` wrapped: the receiver would
  read a short message and then treat the remainder as the next frame's header,
  desynchronising the stream. `decode_message` already enforced
  `MAX_MESSAGE_SIZE`; the encode side enforced nothing.

## [0.0.1] - 2026-02-21

### Added

- Initial release extracted from crucible workspace
- gRPC message framing (length-prefixed)
- Unary and streaming RPC support (client and server)
- gRPC status codes and metadata
- Timeout support
- Re-exports of HTTP/2 types from http2-proto
