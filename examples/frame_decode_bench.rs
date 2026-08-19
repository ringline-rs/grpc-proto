use bytes::BytesMut;
use grpc_proto::{decode_message, encode_message};
use std::env;
use std::time::Instant;

const DEFAULT_ITERATIONS: u64 = 1_000_000;
const SEED: u64 = 0x6b61_6e69_6772_7063;
const HEADER_SIZE: usize = 5;
const MAX_MESSAGE_SIZE: usize = 4 * 1024 * 1024;
const MAX_PAYLOAD: usize = 8;
const MAX_TRAILING: usize = 3;
const MAX_INPUT: usize = HEADER_SIZE + MAX_PAYLOAD + MAX_TRAILING;

#[derive(Clone, Copy)]
enum OutcomeClass {
    IncompleteHeader,
    IncompleteBody,
    Oversized,
    Success,
}

impl OutcomeClass {
    const fn index(self) -> usize {
        match self {
            Self::IncompleteHeader => 0,
            Self::IncompleteBody => 1,
            Self::Oversized => 2,
            Self::Success => 3,
        }
    }
}

struct Case {
    input: BytesMut,
    class: OutcomeClass,
    invalid_flag: bool,
}

fn next_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn put_header(input: &mut [u8], flag: u8, length: u32) {
    input[0] = flag;
    input[1..HEADER_SIZE].copy_from_slice(&length.to_be_bytes());
}

fn classify_case(input: &[u8]) -> OutcomeClass {
    if input.len() < HEADER_SIZE {
        return OutcomeClass::IncompleteHeader;
    }
    let length = u32::from_be_bytes([input[1], input[2], input[3], input[4]]) as usize;
    if length > MAX_MESSAGE_SIZE {
        OutcomeClass::Oversized
    } else if input.len() < HEADER_SIZE + length {
        OutcomeClass::IncompleteBody
    } else {
        OutcomeClass::Success
    }
}

fn generate_case(scenario: usize, state: &mut u64) -> Case {
    let mut bytes = [0_u8; MAX_INPUT];
    let (input_len, class) = match scenario % 5 {
        0 => {
            let input_len = next_random(state) as usize % HEADER_SIZE;
            (input_len, OutcomeClass::IncompleteHeader)
        }
        1 => {
            let length = 1 + next_random(state) as usize % MAX_PAYLOAD;
            let present = next_random(state) as usize % length;
            put_header(&mut bytes, (next_random(state) & 1) as u8, length as u32);
            (HEADER_SIZE + present, OutcomeClass::IncompleteBody)
        }
        2 => {
            let length = MAX_MESSAGE_SIZE as u32 + 1 + next_random(state) as u32 % 1024;
            put_header(&mut bytes, next_random(state) as u8, length);
            (HEADER_SIZE, OutcomeClass::Oversized)
        }
        3 => {
            let length = next_random(state) as usize % (MAX_PAYLOAD + 1);
            let trailing = next_random(state) as usize % (MAX_TRAILING + 1);
            for byte in &mut bytes[HEADER_SIZE..HEADER_SIZE + length + trailing] {
                *byte = next_random(state) as u8;
            }
            put_header(&mut bytes, (next_random(state) & 1) as u8, length as u32);
            (HEADER_SIZE + length + trailing, OutcomeClass::Success)
        }
        _ => {
            let input_len = next_random(state) as usize % (MAX_INPUT + 1);
            for byte in &mut bytes[..input_len] {
                *byte = next_random(state) as u8;
            }
            if input_len > 0 {
                bytes[0] |= 2;
            }
            (input_len, classify_case(&bytes[..input_len]))
        }
    };

    let input = if matches!(class, OutcomeClass::Success) && scenario % 5 == 3 {
        let length = u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]) as usize;
        let trailing = &bytes[HEADER_SIZE + length..input_len];
        let mut encoded =
            BytesMut::from(&encode_message(&bytes[HEADER_SIZE..HEADER_SIZE + length])[..]);
        encoded[0] = bytes[0];
        encoded.extend_from_slice(trailing);
        encoded
    } else {
        BytesMut::from(&bytes[..input_len])
    };

    Case {
        invalid_flag: input.first().is_some_and(|flag| *flag > 1),
        input,
        class,
    }
}

fn main() {
    let iterations = env::args()
        .nth(1)
        .map(|value| value.parse().expect("iterations must be an integer"))
        .unwrap_or(DEFAULT_ITERATIONS);
    assert!(iterations > 0, "iterations must be greater than zero");

    let mut state = SEED;
    let mut counts = [0_u64; 4];
    let mut invalid_flags = 0_u64;
    let started = Instant::now();

    for iteration in 0..iterations {
        let mut case = generate_case(iteration as usize, &mut state);
        let before = case.input.clone();
        counts[case.class.index()] += 1;
        invalid_flags += u64::from(case.invalid_flag);

        let result = decode_message(&mut case.input);
        match case.class {
            OutcomeClass::IncompleteHeader | OutcomeClass::IncompleteBody => {
                assert!(result.expect("incomplete frame must not error").is_none());
                assert_eq!(case.input, before);
            }
            OutcomeClass::Oversized => {
                assert_eq!(
                    result.expect_err("oversized frame must error").kind(),
                    std::io::ErrorKind::InvalidData
                );
                assert_eq!(case.input, before);
            }
            OutcomeClass::Success => {
                let declared =
                    u32::from_be_bytes([before[1], before[2], before[3], before[4]]) as usize;
                let (payload, compressed) = result
                    .expect("bounded complete frame must not error")
                    .expect("bounded complete frame must decode");
                assert_eq!(
                    payload.as_ref(),
                    &before[HEADER_SIZE..HEADER_SIZE + declared]
                );
                assert_eq!(case.input.as_ref(), &before[HEADER_SIZE + declared..]);
                assert_eq!(before.len() - case.input.len(), HEADER_SIZE + declared);
                if !case.invalid_flag {
                    assert_eq!(compressed, before[0] == 1);
                }
            }
        }
    }

    let elapsed = started.elapsed();
    let rate = iterations as f64 / elapsed.as_secs_f64();
    println!(
        "iterations={iterations} seed=0x{SEED:016x} elapsed={:.6}s cases_per_second={rate:.0}",
        elapsed.as_secs_f64()
    );
    println!(
        "incomplete_header={} incomplete_body={} oversized={} success={} invalid_flags={invalid_flags}",
        counts[0], counts[1], counts[2], counts[3]
    );
    println!(
        "domain=input_bytes:0..={MAX_INPUT},valid_payload:0..={MAX_PAYLOAD},trailing:0..={MAX_TRAILING}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_cases_cover_every_decoder_result_class_and_invalid_flags() {
        let mut state = SEED;
        let mut classes = [0_u64; 4];
        let mut saw_invalid_flag = false;

        for scenario in 0..5 {
            let case = generate_case(scenario, &mut state);
            classes[case.class.index()] += 1;
            saw_invalid_flag |= case.invalid_flag;
        }

        assert!(classes.into_iter().all(|count| count > 0));
        assert!(saw_invalid_flag);
    }
}
