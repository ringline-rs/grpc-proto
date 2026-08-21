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

The deterministic comparison runner uses the same 16-byte input bound. It
stratifies samples across short headers, incomplete bodies, oversized lengths,
valid successes with trailing bytes, and arbitrary raw inputs (including
invalid compression flags):

```console
cargo run --release --example frame_decode_bench -- 1000000
```

Results below were measured on the development host and are indicative rather
than a performance guarantee:

<!-- BENCHMARK_RESULTS -->

- Kani 0.67.0, warm build plus four proofs: 10.65 seconds. Individual CBMC
  verification times were 0.25--0.54 seconds per harness. The initial cold run
  before the final proof refinement took 16.46 seconds.
- Seeded runner, already compiled: 1,000,000 cases in 0.080 seconds
  (12,547,505 cases/second): 258,626 short headers, 200,110 incomplete
  bodies, 341,264 oversized frames, and 200,000 successes. The run included
  386,724 inputs with invalid compression flags.
- Seed: `0x6b616e6967727063`; release build; development host on 2026-08-18.

The runner checks preservation for incomplete/error results and exact payload,
consumption, compression (for flags 0 and 1), and trailing bytes for successes.
It measures real `BytesMut` allocation and mutation at high throughput, but one
million seeded cases remain samples. Kani is slower because it exhaustively
explores all bounded classifications and proves the assertions; it intentionally
models the pure classifier rather than `BytesMut` allocation internals.

## Existing semantics outside the proof contract

- Decoding currently treats every nonzero compression flag as compressed. Only
  the panic proof and random runner exercise invalid values; neither asserts a
  desired compression meaning for them. Semantic proofs assume valid flags 0
  and 1.
- Encoding rejects inputs larger than `u32::MAX` before constructing the header,
  because the wire length cannot represent them. The bounded roundtrip proof
  covers every encoded length in its 0-through-8-byte payload domain; a concrete
  test confirms the checked conversion does not reject sizes through the decoder
  limit.
