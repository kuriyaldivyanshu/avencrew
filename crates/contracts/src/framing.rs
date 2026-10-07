//! Incremental u32 big-endian framing. No socket, thread or dispatch ownership.
pub use crate::json::MAX_FRAME_BYTES;
use crate::{decode_internal_message, encode_message, wire::WireMessage, ContractError};

#[derive(Debug, Default)]
pub struct FrameDecoder {
    header: [u8; 4],
    header_len: usize,
    expected: usize,
    payload: Vec<u8>,
    failed: bool,
}
impl FrameDecoder {
    /// Consume at most one frame, returning the consumed byte count. Callers
    /// retain any suffix and feed it again. Memory stays bounded to one frame.
    /// A returned message is internal wire data, never permission to dispatch.
    pub fn feed(&mut self, input: &[u8]) -> Result<(usize, Option<WireMessage>), ContractError> {
        if self.failed {
            return Err(ContractError::DecoderFailed);
        }
        let result = self.consume(input);
        if result.is_err() {
            self.failed = true;
            self.payload.clear();
        }
        result
    }
    fn consume(&mut self, input: &[u8]) -> Result<(usize, Option<WireMessage>), ContractError> {
        let mut used = 0;
        if self.header_len < 4 {
            let count = (4 - self.header_len).min(input.len());
            self.header[self.header_len..self.header_len + count].copy_from_slice(&input[..count]);
            self.header_len += count;
            used += count;
            if self.header_len < 4 {
                return Ok((used, None));
            }
            self.expected = u32::from_be_bytes(self.header) as usize;
            if self.expected == 0 || self.expected > MAX_FRAME_BYTES {
                return Err(ContractError::Limit("frame bytes"));
            }
        }
        let count = (self.expected - self.payload.len()).min(input.len() - used);
        self.payload.extend_from_slice(&input[used..used + count]);
        used += count;
        if self.payload.len() < self.expected {
            return Ok((used, None));
        }
        let message = decode_internal_message(&self.payload)?;
        self.header_len = 0;
        self.expected = 0;
        self.payload.clear();
        Ok((used, Some(message)))
    }
    /// At stream EOF, any partial header or body is an error, never a message.
    pub fn finish(&self) -> Result<(), ContractError> {
        if self.failed {
            return Err(ContractError::DecoderFailed);
        }
        if self.header_len != 0 {
            return Err(ContractError::IncompleteFrame);
        }
        Ok(())
    }
}
pub fn encode_frame(message: &WireMessage) -> Result<Vec<u8>, ContractError> {
    let payload = encode_message(message)?;
    let mut frame = Vec::with_capacity(payload.len() + 4);
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}
