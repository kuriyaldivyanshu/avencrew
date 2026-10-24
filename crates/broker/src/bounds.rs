use crate::BrokerError;
use serde::Serialize;
use serde_json::Value;
use std::io::{self, Write};

/// Reject excessive nesting before serde recursively traverses direct Values.
pub(crate) fn depth(value: &Value) -> Result<(), BrokerError> {
    fn visit(value: &Value, level: usize, nodes: &mut usize) -> Result<(), BrokerError> {
        *nodes += 1;
        if level > 64 || *nodes > 131_072 {
            return Err(BrokerError::LimitExceeded);
        }
        match value {
            Value::Array(items) => {
                for item in items {
                    visit(item, level + 1, nodes)?;
                }
            }
            Value::Object(items) => {
                for item in items.values() {
                    visit(item, level + 1, nodes)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    visit(value, 0, &mut 0)
}
struct BoundedBytes {
    bytes: Vec<u8>,
    max: usize,
}
impl Write for BoundedBytes {
    fn write(&mut self, value: &[u8]) -> io::Result<usize> {
        self.bytes
            .len()
            .checked_add(value.len())
            .filter(|n| *n <= self.max)
            .ok_or_else(|| io::Error::other("broker byte limit"))?;
        self.bytes.extend_from_slice(value);
        Ok(value.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(crate) fn encode<T: Serialize + ?Sized>(value: &T, max: usize) -> Result<Vec<u8>, BrokerError> {
    let mut sink = BoundedBytes {
        bytes: Vec::new(),
        max,
    };
    serde_json::to_writer(&mut sink, value).map_err(|_| BrokerError::LimitExceeded)?;
    Ok(sink.bytes)
}
