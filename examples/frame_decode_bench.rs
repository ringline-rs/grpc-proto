use bytes::BytesMut;
use grpc_proto::{decode_message, encode_message};
use std::env;
use std::time::Instant;

const DEFAULT_ITERATIONS: u64 = 1_000_000;
const SEED: u64 = 0x6b61_6e69_6772_7063;
const MAX_PAYLOAD: usize = 8;
const MAX_TRAILING: usize = 3;

fn next_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn main() {
    let iterations = env::args()
        .nth(1)
        .map(|value| value.parse().expect("iterations must be an integer"))
        .unwrap_or(DEFAULT_ITERATIONS);
    assert!(iterations > 0, "iterations must be greater than zero");

    let mut state = SEED;
    let started = Instant::now();

    for _ in 0..iterations {
        let payload_len = next_random(&mut state) as usize % (MAX_PAYLOAD + 1);
        let trailing_len = next_random(&mut state) as usize % (MAX_TRAILING + 1);
        let compressed = next_random(&mut state) & 1 == 1;

        let mut payload = [0_u8; MAX_PAYLOAD];
        for byte in &mut payload[..payload_len] {
            *byte = next_random(&mut state) as u8;
        }

        let mut input = BytesMut::from(&encode_message(&payload[..payload_len])[..]);
        input[0] = u8::from(compressed);

        let mut trailing = [0_u8; MAX_TRAILING];
        for byte in &mut trailing[..trailing_len] {
            *byte = next_random(&mut state) as u8;
        }
        input.extend_from_slice(&trailing[..trailing_len]);

        let before_len = input.len();
        let (decoded, decoded_compressed) = decode_message(&mut input)
            .expect("valid bounded frame must not error")
            .expect("valid bounded frame must be complete");

        assert_eq!(decoded.as_ref(), &payload[..payload_len]);
        assert_eq!(decoded_compressed, compressed);
        assert_eq!(before_len - input.len(), 5 + payload_len);
        assert_eq!(input.as_ref(), &trailing[..trailing_len]);
    }

    let elapsed = started.elapsed();
    let rate = iterations as f64 / elapsed.as_secs_f64();
    println!(
        "iterations={iterations} seed=0x{SEED:016x} elapsed={:.6}s frames_per_second={rate:.0}",
        elapsed.as_secs_f64()
    );
    println!("domain=payload:0..={MAX_PAYLOAD},trailing:0..={MAX_TRAILING},flag:0|1");
}
