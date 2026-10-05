//! Export canonical types; P1-04 owns the package layout and regeneration command.
use avencrew_contracts::wire::{InputDisposition, WireMessage};
use ts_rs::TS;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let directory = args
        .next()
        .ok_or("usage: export_typescript <output-directory>")?;
    if args.next().is_some() {
        return Err("usage: export_typescript <output-directory>".into());
    }
    let cfg = ts_rs::Config::default()
        .with_out_dir(directory)
        .with_import_extension(Some("js"));
    WireMessage::export_all(&cfg)?;
    InputDisposition::export_all(&cfg)?;
    Ok(())
}
