//! Q02 bounded UTF-8 output formatter.
//!
//! Candidate implementation for the frozen `Q02-v1.1` contract. This file is the
//! only path a candidate may write; `crate::dto` owns every public type, error
//! and constant.
//!
//! The contract's own words, which drive the structure below:
//!
//!   "Bound counter validation before allocation/parsing; borrow and preserve the
//!    input."  Validation is therefore total and ordered, and runs before any
//!    decode. Nothing is allocated from the counter strings: they are validated
//!    by digit inspection and accumulated with checked integer arithmetic.
//!
//!   "No floating-point counter conversion or lossy normalization."  Counters are
//!    read as canonical ASCII decimal with integer-only accumulation. A prior
//!    `offset`/`total_bytes` is never reformatted, only carried; `next_offset` is
//!    composed from exact integers.
//!
//!   "The whole supplied chunk must be valid UTF-8, even with zero limit or
//!    invalid bytes beyond the limit."  Validation covers the entire chunk, not
//!    just the part the limit would select, so loss can never be reported as a
//!    clean short read.
//!
//!   "Loss is never an empty success."  Anything that cannot be accounted for
//!    returns a typed `FormatError` rather than an empty observation.

use crate::dto::{
    BoundedObservation, CounterField, FormatError, LimitBytes, ObservationChunk, MAX_BYTES,
    MAX_COUNTER,
};

/// Longest canonical decimal counter accepted: `MAX_COUNTER` has 19 digits.
const MAX_COUNTER_DIGITS: usize = 19;

/// Validate one canonical ASCII decimal counter and return its value.
///
/// Canonical means exactly `0`, or `[1-9][0-9]*`. Leading zeros, a sign,
/// whitespace, non-ASCII digits, an empty string, more than 19 digits, and any
/// value above `MAX_COUNTER` are all rejected.
///
/// Integer accumulation is checked, so no value can wrap and no parse can panic.
fn parse_counter(raw: &str, field: CounterField) -> Result<u64, FormatError> {
    let invalid = || FormatError::InvalidCounter { field };
    let digits = raw.as_bytes();

    if digits.is_empty() || digits.len() > MAX_COUNTER_DIGITS {
        return Err(invalid());
    }

    // A leading `0` is canonical only as the entire value.
    match digits[0] {
        b'0' if digits.len() == 1 => {}
        b'1'..=b'9' => {}
        _ => return Err(invalid()),
    }

    let mut value: u64 = 0;
    for &digit in digits {
        if !digit.is_ascii_digit() {
            return Err(invalid());
        }
        value = value
            .checked_mul(10)
            .and_then(|scaled| scaled.checked_add(u64::from(digit - b'0')))
            .ok_or_else(invalid)?;
    }

    if value > MAX_COUNTER {
        return Err(invalid());
    }
    Ok(value)
}

/// Select the longest prefix of `text` of at most `cap` bytes that ends on a
/// UTF-8 character boundary, and return its length.
///
/// Stopping before a character that would straddle the limit is the whole point:
/// a partial sequence must never be emitted, replaced, or silently dropped.
fn longest_boundary_prefix(text: &str, cap: usize) -> usize {
    let mut end = cap;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

/// Format one bounded observation from an already-read chunk.
///
/// Validation order is fixed by the contract and is observable through the
/// returned error, so it is applied in exactly that order.
pub fn format_observation(input: &ObservationChunk) -> Result<BoundedObservation, FormatError> {
    // 1. offset  2. total_bytes — counters before any limit or range work.
    let offset = parse_counter(&input.offset, CounterField::Offset)?;
    let total = parse_counter(&input.total_bytes, CounterField::TotalBytes)?;

    // 3. limit_bytes is a native input bound, not an authoritative counter.
    let limit: LimitBytes = input.limit_bytes;
    if limit > MAX_BYTES {
        return Err(FormatError::LimitExceeded);
    }

    // 4. Chunk length bound.
    let chunk_len = input.utf8_bytes.len();
    if chunk_len as u64 > MAX_BYTES {
        return Err(FormatError::ChunkTooLarge);
    }

    // 5. Contiguous range. A preexisting gap does not relax these bounds.
    let end_of_chunk = offset
        .checked_add(chunk_len as u64)
        .ok_or(FormatError::RangeOutOfBounds)?;
    if offset > total || end_of_chunk > total {
        return Err(FormatError::RangeOutOfBounds);
    }

    // 6. Whole-chunk UTF-8 validation, before any selection.
    let text = std::str::from_utf8(&input.utf8_bytes).map_err(|_| FormatError::InvalidUtf8)?;

    let cap = (limit as usize).min(chunk_len);
    let selected = longest_boundary_prefix(text, cap);

    // `selected <= chunk_len` and `end_of_chunk <= total <= MAX_COUNTER`, so this
    // addition cannot overflow; it is still checked rather than assumed.
    let next_offset = offset
        .checked_add(selected as u64)
        .ok_or(FormatError::RangeOutOfBounds)?;

    Ok(BoundedObservation {
        // Exact decoded prefix. No normalization, no BOM or NUL stripping.
        text: text[..selected].to_owned(),
        // Actual bytes consumed, never the limit and never a character count.
        next_offset: next_offset.to_string(),
        // The validated original string, carried unchanged: this chunk may cover
        // only part of the stream.
        total_bytes: input.total_bytes.clone(),
        // Exactly whether bytes remain unconsumed in this chunk. Unread bytes
        // elsewhere in the stream do not imply truncation.
        truncated: selected < chunk_len,
        // Exactly the input flag. Limit truncation is not loss.
        gap: input.gap,
    })
}