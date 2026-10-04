use crate::dto::{BoundedObservation, CounterField, FormatError, ObservationChunk};
use crate::format::format_observation;

fn input(bytes: &[u8], offset: &str, total: &str, limit: u64, gap: bool) -> ObservationChunk {
    ObservationChunk {
        utf8_bytes: bytes.to_vec(), offset: offset.into(), total_bytes: total.into(),
        limit_bytes: limit, gap,
    }
}

fn output(text: &str, next: &str, total: &str, truncated: bool, gap: bool) -> BoundedObservation {
    BoundedObservation {
        text: text.into(), next_offset: next.into(), total_bytes: total.into(), truncated, gap,
    }
}

#[test]
fn empty() {
    assert_eq!(format_observation(&input(b"", "0", "0", 16, false)), Ok(output("", "0", "0", false, false)));
}
#[test]
fn exact_limit() {
    assert_eq!(format_observation(&input(b"hello", "0", "5", 5, false)), Ok(output("hello", "5", "5", false, false)));
}
#[test]
fn over_limit() {
    assert_eq!(format_observation(&input(b"hello world", "0", "11", 5, false)), Ok(output("hello", "5", "11", true, false)));
}
#[test]
fn multibyte_boundary() {
    assert_eq!(format_observation(&input("aé".as_bytes(), "0", "3", 2, false)), Ok(output("a", "1", "3", true, false)));
}
#[test]
fn preexisting_gap() {
    assert_eq!(format_observation(&input(b"hello", "0", "5", 5, true)), Ok(output("hello", "5", "5", false, true)));
}
#[test]
fn nonzero_offset() {
    assert_eq!(format_observation(&input(b"world", "6", "11", 5, false)), Ok(output("world", "11", "11", false, false)));
}
#[test]
fn zero_limit() {
    assert_eq!(format_observation(&input(b"a", "0", "1", 0, false)), Ok(output("", "0", "1", true, false)));
}
#[test]
fn invalid_utf8_is_error() {
    assert_eq!(format_observation(&input(&[0xff], "0", "1", 1, false)), Err(FormatError::InvalidUtf8));
}
#[test]
fn noncanonical_counter_is_error() {
    assert_eq!(format_observation(&input(b"", "00", "0", 0, false)), Err(FormatError::InvalidCounter { field: CounterField::Offset }));
}
