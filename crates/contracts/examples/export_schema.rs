//! Prints the canonical wire schema; callers choose the destination.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema = avencrew_contracts::wire_schema();
    println!("{}", serde_json::to_string_pretty(&schema)?);
    Ok(())
}
