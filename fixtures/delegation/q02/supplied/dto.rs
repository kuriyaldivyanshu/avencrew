//! Q02 contract v1.1, frozen by Sol. Candidate may not edit this file.
pub const MAX_BYTES: u64 = 262_144;
pub const MAX_COUNTER: u64 = i64::MAX as u64;
pub type DecimalCounter = String;
pub type Utf8Bytes = Vec<u8>;
pub type LimitBytes = u64;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationChunk {
    pub utf8_bytes: Utf8Bytes,
    pub offset: DecimalCounter,
    pub total_bytes: DecimalCounter,
    pub limit_bytes: LimitBytes,
    pub gap: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedObservation {
    pub text: String,
    pub next_offset: DecimalCounter,
    pub total_bytes: DecimalCounter,
    pub truncated: bool,
    pub gap: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterField { Offset, TotalBytes }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatError {
    InvalidCounter { field: CounterField },
    LimitExceeded,
    ChunkTooLarge,
    RangeOutOfBounds,
    InvalidUtf8,
}
