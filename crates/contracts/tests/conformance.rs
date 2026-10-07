use avencrew_contracts::{
    canonical_bytes, decode_client_request, decode_internal_message, encode_message,
    framing::{encode_frame, FrameDecoder, MAX_FRAME_BYTES},
    scalars::{Counter, DomainId, Instant, Money, Text},
    ContractError,
};
use serde_json::{json, Value};

fn examples() -> Vec<Value> {
    serde_json::from_str(include_str!("wire-examples.json")).unwrap()
}
fn client() -> Value {
    json!({"schema_version":"1.0","message_kind":"rpc_request","payload":{
        "frame_kind":"request","request_id":"019a5500-0000-7000-8000-000000000001",
        "operation":"describe_capabilities","body":{}}})
}
fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}

#[test]
fn documented_envelopes_round_trip_without_dropping_fields() {
    for example in examples() {
        let input = &example["message"];
        let decoded = decode_internal_message(&bytes(input))
            .unwrap_or_else(|e| panic!("{}: {e}", example["name"]));
        let encoded: Value = serde_json::from_slice(&encode_message(&decoded).unwrap()).unwrap();
        assert_eq!(encoded, *input, "{}", example["name"]);
    }
}
#[test]
fn strict_json_rejects_duplicates_at_any_depth_and_escaped_equivalents() {
    let valid = String::from_utf8(bytes(&client())).unwrap();
    for bad in [
        valid.replace("\"body\":{}", "\"body\":{\"x\":1,\"x\":2}"),
        valid.replace(
            "\"schema_version\":\"1.0\"",
            "\"schema_version\":\"1.0\",\"schema_\\u0076ersion\":\"1.0\"",
        ),
    ] {
        assert!(decode_internal_message(bad.as_bytes()).is_err());
    }
    assert!(canonical_bytes(br#"{"nested":{"key":1,"key":2}}"#).is_err());
}
#[test]
fn unknown_tags_fields_and_versions_are_rejected() {
    let mut value = client();
    value["schema_version"] = json!("2.0");
    assert_eq!(
        decode_internal_message(&bytes(&value)),
        Err(ContractError::IncompatibleVersion)
    );
    let mut value = client();
    value["extra"] = json!(true);
    assert!(decode_internal_message(&bytes(&value)).is_err());
    let mut value = client();
    value["payload"]["operation"] = json!("do_anything");
    assert!(decode_internal_message(&bytes(&value)).is_err());
    let mut value = client();
    value["payload"]["body"]["principal_id"] = json!("forged");
    assert!(decode_client_request(&bytes(&value)).is_err());
    assert!(decode_internal_message(&[0xff]).is_err());
    assert!(decode_internal_message(b"{}").is_err());
    let mut trailing = bytes(&client());
    trailing.extend_from_slice(b" {}");
    assert!(decode_internal_message(&trailing).is_err());
}
#[test]
fn caller_cannot_use_internal_control_as_a_client_request() {
    for example in examples() {
        let message = &example["message"];
        if message["message_kind"] == "rpc_request" {
            let internal = message["payload"]["operation"]
                .as_str()
                .unwrap()
                .starts_with("harness.");
            assert_eq!(
                decode_client_request(&bytes(message)).is_err(),
                internal,
                "{}",
                example["name"]
            );
        } else {
            assert!(decode_client_request(&bytes(message)).is_err());
        }
    }
}
#[test]
fn scalars_preserve_precision_and_validate_calendar_and_spelling() {
    for good in ["0", "9007199254740993", "9223372036854775807"] {
        let counter = Counter::new(good).unwrap();
        assert_eq!(serde_json::to_value(counter).unwrap(), json!(good));
    }
    for bad in ["01", "+1", "-1", "1.0", "1e2", "9223372036854775808", ""] {
        assert!(Counter::new(bad).is_err());
    }
    assert!(serde_json::from_str::<Counter>("9007199254740993").is_err());
    assert!(DomainId::new("019A5500-0000-7000-8000-000000000001").is_err());
    assert!(DomainId::new("019a5500-0000-4000-8000-000000000001").is_err());
    assert!(Instant::new("2026-02-30T00:00:00.000000Z").is_err());
    assert!(Instant::new("2026-10-06T00:00:00.000Z").is_err());
    assert!(Instant::new("2026-10-06T00:00:00.000000+00:00").is_err());
    assert!(Instant::new("2026-10-06T23:59:60.000000Z").is_err());
    assert_eq!(
        Instant::new("1970-01-01T00:00:00.000001Z")
            .unwrap()
            .unix_micros(),
        1
    );
    assert!(Money::new("12.000001").is_ok());
    assert!(Money::new("12.00001").is_err());
    assert!(Text::<0, 2>::new("é😀").is_ok());
    assert!(Text::<0, 2>::new("é😀x").is_err());
}
#[test]
fn null_must_be_present_and_status_union_must_match() {
    let all = examples();
    let mut lease = all
        .iter()
        .find(|v| v["message"]["message_kind"] == "lease")
        .unwrap()["message"]
        .clone();
    assert!(decode_internal_message(&bytes(&lease)).is_ok());
    lease["payload"]
        .as_object_mut()
        .unwrap()
        .remove("environment_id");
    assert!(decode_internal_message(&bytes(&lease)).is_err());
    let mut result = all
        .iter()
        .find(|v| v["message"]["message_kind"] == "tool_result")
        .unwrap()["message"]
        .clone();
    result["payload"]["status"] = json!("denied");
    assert!(decode_internal_message(&bytes(&result)).is_err());
    let mut receipt = all
        .iter()
        .find(|v| v["message"]["payload"]["kind"] == "accepted")
        .unwrap()["message"]
        .clone();
    receipt["payload"]["kind"] = json!("applied");
    assert!(decode_internal_message(&bytes(&receipt)).is_err());
}
#[test]
fn semantic_cursor_and_environment_pairs_are_checked() {
    let all = examples();
    let mut event = all
        .iter()
        .find(|v| v["message"]["message_kind"] == "event")
        .unwrap()["message"]
        .clone();
    event["payload"]["epoch"] = json!("0");
    assert!(decode_internal_message(&bytes(&event)).is_ok());
    let snapshot = all
        .iter()
        .find(|v| {
            v["message"]["payload"]["operation"] == "get_task_snapshot"
                && v["message"]["payload"]["ok"] == true
        })
        .map(|v| v["message"].clone());
    let mut value = snapshot.unwrap();
    value["payload"]["result"]["applied_seq"] = json!("2");
    assert!(decode_internal_message(&bytes(&value)).is_err());
    let mut value = all
        .iter()
        .find(|v| v["message"]["message_kind"] == "lease")
        .unwrap()["message"]
        .clone();
    value["payload"]["generation"] = json!("1");
    assert!(decode_internal_message(&bytes(&value)).is_err());
    value["payload"]["generation"] = Value::Null;
    value["payload"]["epoch"] = json!("0");
    assert!(decode_internal_message(&bytes(&value)).is_err());
    let mut lease = all
        .iter()
        .find(|v| v["message"]["message_kind"] == "lease")
        .unwrap()["message"]
        .clone();
    lease["payload"]["authority"] = json!("server");
    assert!(
        decode_internal_message(&bytes(&lease)).is_ok(),
        "server authority can run on a laptop"
    );
    lease["payload"]["authority"] = json!("local");
    lease["payload"]["placement"] = json!("cloud");
    assert!(decode_internal_message(&bytes(&lease)).is_err());
    let mut transfer = all
        .iter()
        .find(|v| {
            v["message"]["payload"]["operation"] == "prepare_transfer"
                && v["message"]["payload"]["ok"] == true
        })
        .unwrap()["message"]
        .clone();
    transfer["payload"]["result"]["disposition"] = json!("committed");
    transfer["payload"]["result"]["authority"] = json!("server");
    transfer["payload"]["result"]["destination_epoch"] = json!("2");
    transfer["payload"]["result"]["receipt_ref"] = json!("019a5500-0000-7000-8000-000000000001");
    assert!(decode_internal_message(&bytes(&transfer)).is_ok());
    transfer["payload"]["result"]["destination_epoch"] = json!("1");
    assert!(decode_internal_message(&bytes(&transfer)).is_err());
}
#[test]
fn bounds_apply_before_a_message_is_returned() {
    let mut value = client();
    value["payload"]["operation"] = json!("submit_task");
    let example = examples()
        .into_iter()
        .find(|v| v["message"]["payload"]["operation"] == "submit_task")
        .unwrap();
    value["payload"]["body"] = example["message"]["payload"]["body"].clone();
    value["payload"]["body"]["objective"] = json!("x".repeat(8193));
    assert!(decode_client_request(&bytes(&value)).is_err());
    value["payload"]["body"]["objective"] = json!("bounded task");
    value["payload"]["body"]["source_refs"] =
        json!(vec!["019a5500-0000-7000-8000-000000000001"; 101]);
    assert!(decode_client_request(&bytes(&value)).is_err());
    value["payload"]["body"]["source_refs"] = json!([]);
    value["payload"]["body"]["lower_limits"]["model_steps"] = json!(51);
    assert!(decode_client_request(&bytes(&value)).is_err());
    let mut response = examples()
        .into_iter()
        .find(|v| v["message"]["message_kind"] == "normalized_model_response")
        .unwrap()["message"]
        .clone();
    response["payload"]["items"] = json!([{"kind":"tool_proposal","provider_call_id":"call-1","binding_id":"019a5500-0000-7000-8000-000000000001","arguments":{"x":"é".repeat((262_144 - 8) / 2)}}]);
    assert!(decode_internal_message(&bytes(&response)).is_ok());
    response["payload"]["items"][0]["arguments"]["x"] = json!("é".repeat((262_144 - 8) / 2) + "x");
    assert_eq!(
        decode_internal_message(&bytes(&response)),
        Err(ContractError::Limit("tool argument bytes"))
    );
    assert!(decode_internal_message(&vec![b' '; MAX_FRAME_BYTES + 1]).is_err());
}
#[test]
fn fragmented_and_adjacent_frames_are_not_dispatched_early() {
    let message = decode_internal_message(&bytes(&client())).unwrap();
    let frame = encode_frame(&message).unwrap();
    let mut decoder = FrameDecoder::default();
    for byte in &frame[..frame.len() - 1] {
        assert_eq!(decoder.feed(&[*byte]).unwrap(), (1, None));
        assert!(decoder.finish().is_err());
    }
    assert_eq!(
        decoder.feed(&frame[frame.len() - 1..]).unwrap(),
        (1, Some(message.clone()))
    );
    assert!(decoder.finish().is_ok());
    let joined = [frame.clone(), frame.clone()].concat();
    let (used, first) = decoder.feed(&joined).unwrap();
    assert_eq!(used, frame.len());
    assert_eq!(first, Some(message.clone()));
    assert_eq!(
        decoder.feed(&joined[used..]).unwrap(),
        (frame.len(), Some(message))
    );
    let mut decoder = FrameDecoder::default();
    assert!(decoder.feed(&[0, 0, 0, 1, 0xff]).is_err());
    assert_eq!(decoder.feed(&frame), Err(ContractError::DecoderFailed));
    for length in [0, (MAX_FRAME_BYTES + 1) as u32] {
        let mut decoder = FrameDecoder::default();
        assert!(decoder.feed(&length.to_be_bytes()).is_err());
        assert_eq!(decoder.feed(&frame), Err(ContractError::DecoderFailed));
    }
}
#[test]
fn canonical_json_is_exact_and_rejects_float_and_duplicate_normalization() {
    assert_eq!(
        canonical_bytes(br#" { "z":2,"a":{"b":1},"text":"e\u0301" } "#).unwrap(),
        "{\"a\":{\"b\":1},\"text\":\"e\u{301}\",\"z\":2}".as_bytes()
    );
    for bad in [
        br#"{"n":1.0}"#.as_slice(),
        br#"{"n":1e0}"#,
        br#"{"a":1,"a":2}"#,
    ] {
        assert!(canonical_bytes(bad).is_err());
    }
}

#[cfg(feature = "schema-export")]
#[test]
fn generated_schema_agrees_with_all_example_shapes() {
    let schema = avencrew_contracts::wire_schema();
    let value = serde_json::to_value(schema).unwrap();
    assert_eq!(
        value["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    let validator = jsonschema::options().offline().build(&value).unwrap();
    for example in examples() {
        assert!(
            validator.is_valid(&example["message"]),
            "{}",
            example["name"]
        );
    }
    let mut invalid = client();
    invalid["payload"]["extra"] = json!(true);
    assert!(!validator.is_valid(&invalid));
    let mut lease = examples()
        .into_iter()
        .find(|v| v["message"]["message_kind"] == "lease")
        .unwrap()["message"]
        .clone();
    lease["payload"]
        .as_object_mut()
        .unwrap()
        .remove("environment_id");
    assert!(!validator.is_valid(&lease));
}

#[cfg(feature = "typescript-export")]
#[test]
fn typescript_keeps_wire_tags_and_precision_scalars() {
    use ts_rs::TS;
    let cfg = ts_rs::Config::default();
    let wire = avencrew_contracts::wire::WireMessage::decl(&cfg);
    assert!(wire.contains("\"message_kind\": \"rpc_request\""), "{wire}");
    assert!(
        !wire.contains("RpcRequest: {"),
        "external enum tagging would change the wire"
    );
    let request = avencrew_contracts::wire::RpcRequest::decl(&cfg);
    assert!(
        request.contains("\"operation\": \"submit_task\""),
        "{request}"
    );
    assert_eq!(Counter::name(&cfg), "string");
    assert_eq!(
        avencrew_contracts::scalars::PositiveCounter::name(&cfg),
        "string"
    );
    assert_eq!(Money::name(&cfg), "string");
}
