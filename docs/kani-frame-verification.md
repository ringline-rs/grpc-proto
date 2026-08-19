# Bounded gRPC frame verification

The frame classifier is exhaustively model checked with Kani 0.67.0. The
proof domain contains every byte sequence up to 16 bytes: a 5-byte header,
payloads up to 8 bytes, and up to 3 trailing bytes. The harnesses establish:

- classification never panics for any bounded byte sequence and input length;
- incomplete and oversized classifications expose no payload extent;
- successful classification identifies exactly `5 + declared_length` bytes,
  the declared payload length, and the compression bit for valid flags 0 and 1;
- headers emitted for every bounded length and compression value classify back
  to the same length and compression value.

Run the exhaustive proofs with:

```console
cargo kani
```

Concrete unit regressions exercise the `BytesMut` boundary. They verify that
incomplete/error results preserve the buffer, while success returns the exact
payload and leaves trailing bytes untouched. Keeping allocator behavior in
concrete tests avoids symbolic allocator-state explosion; Kani covers the pure
header decision that controls those mutations.

## Reproducible random comparison

The deterministic comparison runner uses the same payload/trailing/flag bounds:

```console
cargo run --release --example frame_decode_bench -- 1000000
```

Results below were measured on the development host and are indicative rather
than a performance guarantee:

<!-- BENCHMARK_RESULTS -->

- Kani 0.67.0, warm build plus four proofs: 10.65 seconds. Individual CBMC
  verification times were 0.25--0.54 seconds per harness. The initial cold run
  before the final proof refinement took 16.46 seconds.
- Seeded runner, already compiled: 1,000,000 frames in 0.101 seconds
  (9,898,655 frames/second).
- Seed: `0x6b616e6967727063`; release build; development host on 2026-08-18.

The runner measures real `BytesMut` allocation and mutation at high throughput,
but samples only one million seeded cases. Kani is slower because it explores
all bounded classifications and proves the assertions; it intentionally models
the pure classifier rather than `BytesMut` allocation internals.

## Existing semantics outside the proof contract

- Decoding currently treats every nonzero compression flag as compressed. The
  semantic proofs assume the valid wire values 0 and 1 and do not endorse other
  values.
- Encoding casts `data.len()` to `u32`. Inputs larger than `u32::MAX` would have
  a truncated wire length. This PR preserves the public infallible API and does
  not claim a roundtrip proof for such inputs.
