//! Emit the authoritative accept/reject verdict for each wire example.
//!
//! P1-04 cross-language conformance: the TypeScript package must agree with this
//! oracle. It reads the same synthetic `tests/wire-examples.json` used by the
//! Rust conformance test, and additionally derives a family of invalid inputs
//! (unknown field, unknown tag, unsupported version, duplicate key, bad counter,
//! bad calendar instant, bounds, and the documented cross-field rules).
//!
//! Output is one JSON document on stdout so the TS runner can compare verdicts
//! without re-implementing Rust. This grants nothing: it calls the same decoders
//! the runtime would, and adds no validation of its own.

use avencrew_contracts::{decode_client_request, decode_internal_message};
use serde_json::{json, Value};

/// Load the shared synthetic examples.
fn examples() -> Vec<Value> {
    serde_json::from_str(include_str!("../tests/wire-examples.json")).expect("wire examples")
}

fn internal(name: &str, bytes: &[u8]) -> Value {
    match decode_internal_message(bytes) {
        Ok(_) => json!({ "case": name, "accept": true }),
        Err(error) => json!({ "case": name, "accept": false, "error": error.to_string() }),
    }
}

fn client(name: &str, value: &Value) -> Value {
    let bytes = serde_json::to_vec(value).expect("serialize");
    match decode_client_request(&bytes) {
        Ok(_) => json!({ "case": name, "accept": true }),
        Err(error) => json!({ "case": name, "accept": false, "error": error.to_string() }),
    }
}

/// Find a synthetic example by message kind, optionally by an operation.
fn find(examples: &[Value], kind: &str, operation: Option<&str>) -> Value {
    examples
        .iter()
        .map(|example| &example["message"])
        .find(|message| {
            message["message_kind"] == json!(kind)
                && operation.is_none_or(|wanted| message["payload"]["operation"] == json!(wanted))
        })
        .cloned()
        .unwrap_or_else(|| panic!("no {kind} example"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Existing conformance runner may submit raw JSON strings, including
    // duplicates and noncanonical numbers, without normalizing them first.
    if std::env::args().nth(1).as_deref() == Some("--stdin") {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Case {
            case: String,
            text: String,
            client: bool,
        }
        let cases: Vec<Case> = serde_json::from_reader(std::io::stdin().lock())?;
        let out: Vec<Value> = cases
            .iter()
            .map(|case| {
                let accept = if case.client {
                    decode_client_request(case.text.as_bytes()).is_ok()
                } else {
                    decode_internal_message(case.text.as_bytes()).is_ok()
                };
                json!({ "case": case.case, "accept": accept })
            })
            .collect();
        println!("{}", serde_json::to_string(&out)?);
        return Ok(());
    }
    let examples = examples();
    let mut out: Vec<Value> = Vec::new();

    // 1. Every documented example, on the internal surface.
    for example in &examples {
        let name = example["name"].as_str().unwrap_or("unnamed");
        let bytes = serde_json::to_vec(&example["message"])?;
        out.push(internal(&format!("valid-internal/{name}"), &bytes));
    }

    // 2. Client surface: harness.* operations must be refused.
    for example in &examples {
        let message = &example["message"];
        let name = example["name"].as_str().unwrap_or("unnamed");
        out.push(client(&format!("client-surface/{name}"), message));
    }

    // 3. Invalid inputs derived from the examples (temporary mutations, no new fixtures).
    let lease = find(&examples, "lease", None);
    let receipt = find(&examples, "command_receipt", None);
    let request = find(&examples, "rpc_request", Some("submit_task"));

    let mut value = lease.clone();
    value["payload"]["epoch"] = json!("01"); // non-canonical leading zero
    out.push(internal(
        "invalid/leading-zero-counter",
        &serde_json::to_vec(&value)?,
    ));

    let mut value = lease.clone();
    value["payload"]["epoch"] = json!("9223372036854775808"); // above int64
    out.push(internal(
        "invalid/counter-out-of-int64",
        &serde_json::to_vec(&value)?,
    ));

    let mut value = lease.clone();
    value["payload"]["expires_at"] = json!("2026-02-30T00:00:00.000000Z"); // impossible date
    out.push(internal(
        "invalid/calendar-date",
        &serde_json::to_vec(&value)?,
    ));

    let mut value = lease.clone();
    value["payload"]["expires_at"] = json!("2026-10-06T00:00:00.000Z"); // wrong fraction digits
    out.push(internal(
        "invalid/instant-fraction",
        &serde_json::to_vec(&value)?,
    ));

    let mut value = lease.clone();
    value["payload"]["receipt_digest"] = json!("AAAA"); // short digest
    out.push(internal(
        "invalid/short-digest",
        &serde_json::to_vec(&value)?,
    ));

    let mut value = lease.clone();
    value["payload"]["unexpected"] = json!(true); // unknown field
    out.push(internal(
        "invalid/unknown-field",
        &serde_json::to_vec(&value)?,
    ));

    let mut value = lease.clone();
    value["payload"]["generation"] = json!("1"); // unpaired environment/generation
    out.push(internal(
        "invalid/unpaired-generation",
        &serde_json::to_vec(&value)?,
    ));

    let mut value = lease.clone();
    value["payload"]["authority"] = json!("server");
    value["payload"]["placement"] = json!("cloud");
    value["payload"]["environment_id"] = json!(null);
    out.push(internal(
        "valid/local-authority-not-cloud",
        &serde_json::to_vec(&value)?,
    ));

    let mut value = receipt.clone();
    value["payload"]["kind"] = json!("nonsense"); // unknown tag
    out.push(internal(
        "invalid/unknown-receipt-tag",
        &serde_json::to_vec(&value)?,
    ));

    let mut value = lease.clone();
    value["schema_version"] = json!("2.0"); // unsupported version
    out.push(internal(
        "invalid/unsupported-version",
        &serde_json::to_vec(&value)?,
    ));

    // duplicate key at the envelope
    let good = "{\"schema_version\":\"1.0\",\"schema_version\":\"1.0\"}";
    out.push(internal("invalid/duplicate-key", good.as_bytes()));

    // non-integer number where a counter string is required
    let mut value = lease.clone();
    value["payload"]["epoch"] = json!(1);
    out.push(internal(
        "invalid/counter-as-number",
        &serde_json::to_vec(&value)?,
    ));

    // A client-surface bounds case: objective over its documented limit.
    let mut bounded = request.clone();
    bounded["payload"]["body"]["objective"] = json!("x".repeat(8193));
    out.push(client("invalid/objective-over-limit", &bounded));
    let mut bounded = request.clone();
    bounded["payload"]["body"]["source_refs"] =
        json!(vec!["019a5500-0000-7000-8000-000000000001"; 101]);
    out.push(client("invalid/source-refs-over-limit", &bounded));
    let mut bounded = request.clone();
    bounded["payload"]["body"]["lower_limits"]["model_steps"] = json!(51);
    out.push(client("invalid/model-steps-over-limit", &bounded));

    println!("{}", serde_json::to_string(&out)?);
    Ok(())
}
