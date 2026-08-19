//! gRPC message framing.
//!
//! gRPC messages are length-prefixed with the following format:
//! - 1 byte: compressed flag (0 = uncompressed, 1 = compressed)
//! - 4 bytes: message length (big-endian u32)
//! - N bytes: message payload

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::io;

/// Size of the gRPC message header (1 byte flag + 4 bytes length).
pub const HEADER_SIZE: usize = 5;

/// Maximum message size (4MB default, matches gRPC default).
pub const MAX_MESSAGE_SIZE: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameClassification {
    Incomplete,
    TooLarge {
        length: usize,
    },
    Complete {
        compressed: bool,
        length: usize,
        total_size: usize,
    },
}

fn encode_header(length: u32, compressed: bool) -> [u8; HEADER_SIZE] {
    let length = length.to_be_bytes();
    [
        u8::from(compressed),
        length[0],
        length[1],
        length[2],
        length[3],
    ]
}

fn classify_frame(buf: &[u8]) -> FrameClassification {
    if buf.len() < HEADER_SIZE {
        return FrameClassification::Incomplete;
    }

    let compressed = buf[0] != 0;
    let length = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;

    if length > MAX_MESSAGE_SIZE {
        return FrameClassification::TooLarge { length };
    }

    let total_size = HEADER_SIZE + length;
    if buf.len() < total_size {
        return FrameClassification::Incomplete;
    }

    FrameClassification::Complete {
        compressed,
        length,
        total_size,
    }
}

/// Encode a message into gRPC wire format.
///
/// Returns the encoded message with the length prefix.
pub fn encode_message(data: &[u8]) -> Bytes {
    encode_message_with_compression(data, false)
}

/// Encode a message with explicit compression flag.
pub fn encode_message_with_compression(data: &[u8], compressed: bool) -> Bytes {
    let mut buf = BytesMut::with_capacity(HEADER_SIZE + data.len());

    buf.put_slice(&encode_header(data.len() as u32, compressed));

    // Message data
    buf.put_slice(data);

    buf.freeze()
}

/// Decode a single message from gRPC wire format.
///
/// Returns `Ok(Some((message, compressed)))` if a complete message was decoded,
/// `Ok(None)` if more data is needed, or `Err` on protocol error.
pub fn decode_message(buf: &mut BytesMut) -> io::Result<Option<(Bytes, bool)>> {
    let (compressed, length) = match classify_frame(buf) {
        FrameClassification::Incomplete => return Ok(None),
        FrameClassification::TooLarge { length } => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("message too large: {} bytes", length),
            ));
        }
        FrameClassification::Complete {
            compressed, length, ..
        } => (compressed, length),
    };

    // Consume header
    buf.advance(HEADER_SIZE);

    // Extract message
    let message = buf.split_to(length).freeze();

    Ok(Some((message, compressed)))
}

/// Stateful decoder for gRPC messages.
///
/// Useful for incrementally decoding messages from a stream.
#[derive(Debug, Default)]
pub struct MessageDecoder {
    /// Buffer for incomplete messages.
    buffer: BytesMut,
}

impl MessageDecoder {
    /// Create a new message decoder.
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::with_capacity(4096),
        }
    }

    /// Feed data into the decoder.
    pub fn feed(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Try to decode the next message.
    ///
    /// Returns `Ok(Some((message, compressed)))` if a complete message was decoded,
    /// `Ok(None)` if more data is needed.
    pub fn decode(&mut self) -> io::Result<Option<(Bytes, bool)>> {
        decode_message(&mut self.buffer)
    }

    /// Check if there's any buffered data.
    pub fn has_buffered_data(&self) -> bool {
        !self.buffer.is_empty()
    }

    /// Get the amount of buffered data.
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_frame_classification_reports_incomplete() {
        let input = [0, 0, 0, 0, 3, 0xaa, 0xbb];

        assert_eq!(classify_frame(&input), FrameClassification::Incomplete);
    }

    #[test]
    fn complete_frame_classification_identifies_exact_payload_extent() {
        let input = [1, 0, 0, 0, 2, 0xaa, 0xbb, 0xcc];

        assert_eq!(
            classify_frame(&input),
            FrameClassification::Complete {
                compressed: true,
                length: 2,
                total_size: 7,
            }
        );
    }

    #[test]
    fn oversized_frame_classification_reports_the_declared_length() {
        let length = (MAX_MESSAGE_SIZE as u32 + 1).to_be_bytes();
        let input = [0, length[0], length[1], length[2], length[3]];

        assert_eq!(
            classify_frame(&input),
            FrameClassification::TooLarge {
                length: MAX_MESSAGE_SIZE + 1,
            }
        );
    }

    #[test]
    fn encoded_header_roundtrips_through_classification() {
        let header = encode_header(3, true);
        let mut input = Vec::from(header);
        input.extend_from_slice(&[0xaa, 0xbb, 0xcc]);

        assert_eq!(
            classify_frame(&input),
            FrameClassification::Complete {
                compressed: true,
                length: 3,
                total_size: 8,
            }
        );
    }

    #[test]
    fn incomplete_decode_preserves_buffer_bytes() {
        let mut input = BytesMut::from(&[0, 0, 0, 0, 3, 0xaa, 0xbb][..]);
        let before = input.clone();

        assert!(decode_message(&mut input).unwrap().is_none());
        assert_eq!(input, before);
    }

    #[test]
    fn oversized_decode_preserves_buffer_bytes() {
        let length = (MAX_MESSAGE_SIZE as u32 + 1).to_be_bytes();
        let mut input = BytesMut::from(&[0, length[0], length[1], length[2], length[3]][..]);
        let before = input.clone();

        assert_eq!(
            decode_message(&mut input).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(input, before);
    }

    #[test]
    fn successful_decode_returns_exact_payload_and_leaves_trailing_bytes() {
        let mut input = BytesMut::from(&[1, 0, 0, 0, 2, 0xaa, 0xbb, 0xcc, 0xdd][..]);

        let (payload, compressed) = decode_message(&mut input).unwrap().unwrap();

        assert_eq!(payload.as_ref(), &[0xaa, 0xbb]);
        assert!(compressed);
        assert_eq!(input.as_ref(), &[0xcc, 0xdd]);
    }

    #[test]
    fn test_encode_empty_message() {
        let encoded = encode_message(&[]);
        assert_eq!(encoded.len(), HEADER_SIZE);
        assert_eq!(encoded[0], 0); // Not compressed
        assert_eq!(&encoded[1..5], &[0, 0, 0, 0]); // Length = 0
    }

    #[test]
    fn test_encode_message() {
        let data = b"hello world";
        let encoded = encode_message(data);

        assert_eq!(encoded.len(), HEADER_SIZE + data.len());
        assert_eq!(encoded[0], 0); // Not compressed
        assert_eq!(
            u32::from_be_bytes([encoded[1], encoded[2], encoded[3], encoded[4]]),
            data.len() as u32
        );
        assert_eq!(&encoded[HEADER_SIZE..], data);
    }

    #[test]
    fn test_encode_compressed() {
        let data = b"test";
        let encoded = encode_message_with_compression(data, true);
        assert_eq!(encoded[0], 1); // Compressed flag
    }

    #[test]
    fn test_decode_complete_message() {
        let data = b"hello world";
        let encoded = encode_message(data);
        let mut buf = BytesMut::from(&encoded[..]);

        let result = decode_message(&mut buf).unwrap();
        assert!(result.is_some());

        let (message, compressed) = result.unwrap();
        assert!(!compressed);
        assert_eq!(&message[..], data);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_decode_incomplete_header() {
        let mut buf = BytesMut::from(&[0, 0, 0][..]);
        let result = decode_message(&mut buf).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_incomplete_body() {
        let data = b"hello world";
        let encoded = encode_message(data);
        // Only provide part of the message
        let mut buf = BytesMut::from(&encoded[..8]);

        let result = decode_message(&mut buf).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_multiple_messages() {
        let msg1 = b"first";
        let msg2 = b"second";

        let mut buf = BytesMut::new();
        buf.extend_from_slice(&encode_message(msg1));
        buf.extend_from_slice(&encode_message(msg2));

        let (decoded1, _) = decode_message(&mut buf).unwrap().unwrap();
        assert_eq!(&decoded1[..], msg1);

        let (decoded2, _) = decode_message(&mut buf).unwrap().unwrap();
        assert_eq!(&decoded2[..], msg2);

        assert!(buf.is_empty());
    }

    #[test]
    fn test_message_decoder() {
        let mut decoder = MessageDecoder::new();

        let msg = b"test message";
        let encoded = encode_message(msg);

        // Feed partial data
        decoder.feed(&encoded[..3]);
        assert!(decoder.decode().unwrap().is_none());

        // Feed rest of data
        decoder.feed(&encoded[3..]);
        let (decoded, _) = decoder.decode().unwrap().unwrap();
        assert_eq!(&decoded[..], msg);
    }

    #[test]
    fn test_message_too_large() {
        let mut buf = BytesMut::new();
        buf.put_u8(0); // Not compressed
        buf.put_u32(MAX_MESSAGE_SIZE as u32 + 1); // Too large

        let result = decode_message(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn test_message_decoder_new() {
        let decoder = MessageDecoder::new();
        assert!(!decoder.has_buffered_data());
        assert_eq!(decoder.buffered_len(), 0);
    }

    #[test]
    fn test_message_decoder_default() {
        let decoder = MessageDecoder::default();
        assert!(!decoder.has_buffered_data());
    }

    #[test]
    fn test_message_decoder_feed() {
        let mut decoder = MessageDecoder::new();
        decoder.feed(&[1, 2, 3, 4, 5]);
        assert!(decoder.has_buffered_data());
        assert_eq!(decoder.buffered_len(), 5);
    }

    #[test]
    fn test_message_decoder_clear() {
        let mut decoder = MessageDecoder::new();
        decoder.feed(&[1, 2, 3, 4, 5]);
        assert!(decoder.has_buffered_data());

        decoder.clear();
        assert!(!decoder.has_buffered_data());
        assert_eq!(decoder.buffered_len(), 0);
    }

    #[test]
    fn test_message_decoder_multiple_feeds() {
        let mut decoder = MessageDecoder::new();
        let msg = b"hello";
        let encoded = encode_message(msg);

        // Feed in multiple small chunks
        for byte in &encoded[..] {
            decoder.feed(&[*byte]);
        }

        let (decoded, _) = decoder.decode().unwrap().unwrap();
        assert_eq!(&decoded[..], msg);
    }

    #[test]
    fn test_message_decoder_debug() {
        let decoder = MessageDecoder::new();
        let debug_str = format!("{:?}", decoder);
        assert!(debug_str.contains("MessageDecoder"));
    }

    #[test]
    fn test_header_size_constant() {
        assert_eq!(HEADER_SIZE, 5);
    }

    #[test]
    fn test_max_message_size_constant() {
        assert_eq!(MAX_MESSAGE_SIZE, 4 * 1024 * 1024);
    }

    #[test]
    fn test_decode_compressed_message() {
        let data = b"compressed data";
        let encoded = encode_message_with_compression(data, true);
        let mut buf = BytesMut::from(&encoded[..]);

        let (message, compressed) = decode_message(&mut buf).unwrap().unwrap();
        assert!(compressed);
        assert_eq!(&message[..], data);
    }
}

#[cfg(kani)]
mod verification {
    use super::*;

    const MAX_PROOF_PAYLOAD: usize = 8;
    const MAX_TRAILING_BYTES: usize = 3;
    const MAX_PROOF_INPUT: usize = HEADER_SIZE + MAX_PROOF_PAYLOAD + MAX_TRAILING_BYTES;

    #[kani::proof]
    fn classify_bounded_input_never_panics() {
        let bytes: [u8; MAX_PROOF_INPUT] = kani::any();
        let available: usize = kani::any();
        kani::assume(available <= MAX_PROOF_INPUT);

        let _ = classify_frame(&bytes[..available]);
    }

    #[kani::proof]
    fn valid_flags_match_bounded_wire_specification() {
        let bytes: [u8; MAX_PROOF_INPUT] = kani::any();
        let available: usize = kani::any();
        kani::assume(available <= MAX_PROOF_INPUT);
        kani::assume(available < HEADER_SIZE || bytes[0] <= 1);

        let classification = classify_frame(&bytes[..available]);
        if available < HEADER_SIZE {
            assert_eq!(classification, FrameClassification::Incomplete);
            return;
        }

        let declared = u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]) as usize;
        if declared > MAX_MESSAGE_SIZE {
            assert_eq!(
                classification,
                FrameClassification::TooLarge { length: declared }
            );
        } else if available < HEADER_SIZE + declared {
            assert_eq!(classification, FrameClassification::Incomplete);
        } else {
            assert_eq!(
                classification,
                FrameClassification::Complete {
                    compressed: bytes[0] != 0,
                    length: declared,
                    total_size: HEADER_SIZE + declared,
                }
            );
        }
    }

    #[kani::proof]
    fn successful_classification_has_exact_payload_extent() {
        let bytes: [u8; MAX_PROOF_INPUT] = kani::any();
        let available: usize = kani::any();
        kani::assume(available <= MAX_PROOF_INPUT);
        kani::assume(available >= HEADER_SIZE);
        kani::assume(bytes[0] <= 1);

        if let FrameClassification::Complete {
            compressed,
            length,
            total_size,
        } = classify_frame(&bytes[..available])
        {
            let declared = u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]) as usize;
            assert_eq!(length, declared);
            assert_eq!(total_size, HEADER_SIZE + declared);
            assert!(total_size <= available);
            assert_eq!(compressed, bytes[0] == 1);
        }
    }

    #[kani::proof]
    fn bounded_header_encode_decode_roundtrip() {
        let length: u32 = kani::any();
        let compressed: bool = kani::any();
        kani::assume(length as usize <= MAX_PROOF_PAYLOAD);

        let header = encode_header(length, compressed);
        let mut frame = [0_u8; HEADER_SIZE + MAX_PROOF_PAYLOAD];
        frame[..HEADER_SIZE].copy_from_slice(&header);
        let frame_len = HEADER_SIZE + length as usize;

        assert_eq!(
            classify_frame(&frame[..frame_len]),
            FrameClassification::Complete {
                compressed,
                length: length as usize,
                total_size: frame_len,
            }
        );
    }
}
