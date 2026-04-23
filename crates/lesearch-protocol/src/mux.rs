//! Binary multiplexing for PTY streams over WebSocket binary frames.
//!
//! Binary frames carry raw terminal data alongside the JSON-RPC text channel.
//! Each binary frame has a fixed 8-byte header:
//!
//! ```text
//! u16 version    (LE, always 1)
//! u16 stream_id  (assigned during agent.spawn)
//! u32 length     (LE, payload byte count)
//! [payload]
//! ```
//!
//! Clients that did not advertise `"binary-mux"` during handshake receive
//! `agent.output` JSON notifications instead.

/// Current binary-mux protocol version.
pub const MUX_VERSION: u16 = 1;

/// Size of the binary-mux header in bytes.
pub const HEADER_SIZE: usize = 8;

/// A decoded binary-mux frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuxFrame {
    /// Stream identifier (assigned at `agent.spawn`).
    pub stream_id: u16,
    /// Raw payload bytes (terminal data).
    pub payload: Vec<u8>,
}

/// Encode a payload into a binary-mux frame.
///
/// The returned `Vec<u8>` is ready to be sent as a WebSocket binary message.
#[must_use]
pub fn encode(stream_id: u16, payload: &[u8]) -> Vec<u8> {
    let len = payload.len();
    let mut buf = Vec::with_capacity(HEADER_SIZE + len);
    buf.extend_from_slice(&MUX_VERSION.to_le_bytes());
    buf.extend_from_slice(&stream_id.to_le_bytes());
    #[allow(clippy::cast_possible_truncation)]
    buf.extend_from_slice(&(len as u32).to_le_bytes());
    buf.extend_from_slice(payload);
    buf
}

/// Decode a binary-mux frame from raw bytes.
///
/// # Errors
///
/// Returns [`crate::ProtocolError::MuxFrameTooShort`] if `data` is shorter
/// than [`HEADER_SIZE`], or [`crate::ProtocolError::MuxVersionUnsupported`]
/// if the version field is not [`MUX_VERSION`].
pub fn decode(data: &[u8]) -> Result<MuxFrame, crate::ProtocolError> {
    if data.len() < HEADER_SIZE {
        return Err(crate::ProtocolError::MuxFrameTooShort {
            expected: HEADER_SIZE,
            actual: data.len(),
        });
    }

    let version = u16::from_le_bytes([data[0], data[1]]);
    if version != MUX_VERSION {
        return Err(crate::ProtocolError::MuxVersionUnsupported(version));
    }

    let stream_id = u16::from_le_bytes([data[2], data[3]]);
    let length = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;

    let available = data.len() - HEADER_SIZE;
    let take = length.min(available);
    let payload = data[HEADER_SIZE..HEADER_SIZE + take].to_vec();

    Ok(MuxFrame { stream_id, payload })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let original = b"hello, terminal";
        let encoded = encode(42, original);
        assert_eq!(encoded.len(), HEADER_SIZE + original.len());

        let frame = decode(&encoded).unwrap();
        assert_eq!(frame.stream_id, 42);
        assert_eq!(frame.payload, original);
    }

    #[test]
    fn decode_too_short() {
        let result = decode(&[0, 1, 2]);
        assert!(result.is_err());
    }

    #[test]
    fn decode_wrong_version() {
        let mut buf = encode(1, b"x");
        // Corrupt version to 99
        buf[0] = 99;
        buf[1] = 0;
        let result = decode(&buf);
        assert!(result.is_err());
    }

    #[test]
    fn encode_empty_payload() {
        let encoded = encode(0, &[]);
        assert_eq!(encoded.len(), HEADER_SIZE);
        let frame = decode(&encoded).unwrap();
        assert!(frame.payload.is_empty());
        assert_eq!(frame.stream_id, 0);
    }
}
