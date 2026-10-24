use avencrew_contracts::{
    decode_internal_message,
    scalars::{Counter, DomainId},
    wire::{
        NormalizedModelResponseFinish as ModelFinish, NormalizedModelResponseItemsItem as Item,
        WireMessage,
    },
};
use avencrew_harness::model::fixture::*;
use serde_json::{json, Value};

fn catalog() -> Value {
    serde_json::from_str(include_str!("../../../fixtures/models/streams.json")).unwrap()
}
fn case(name: &str) -> FixtureInput {
    let catalog = catalog();
    let events = &catalog["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap()["events"];
    decode_fixture(
        &serde_json::to_vec(&json!({
            "schema_version": catalog["input_schema_version"],
            "seed": catalog["seed"], "events": events,
        }))
        .unwrap(),
    )
    .unwrap()
}
fn input(bodies: Vec<FixtureEventBody>) -> FixtureInput {
    let mut input = case("candidate_only");
    input.events = bodies
        .into_iter()
        .enumerate()
        .map(|(i, body)| FixtureEvent {
            sequence: Counter::new((i + 1).to_string()).unwrap(),
            body,
        })
        .collect();
    input
}
fn result(input: &FixtureInput) -> FixtureResult {
    normalize_fixture(input).unwrap()
}
fn error(input: &FixtureInput, code: FixtureError) -> FixtureFailure {
    let Err(failure) = normalize_fixture(input) else {
        panic!("unexpected success");
    };
    assert_eq!(failure.code, code);
    failure
}
fn both_error(input: &FixtureInput, code: FixtureError) {
    error(input, code);
    let bytes = serde_json::to_vec(input).unwrap();
    match decode_fixture(&bytes) {
        Ok(decoded) => {
            error(&decoded, code);
        }
        Err(failure) => assert_eq!(failure.code, code),
    }
}
fn id(index: usize) -> DomainId {
    DomainId::new(format!("019a5500-0000-7000-8000-{index:012x}")).unwrap()
}
fn tool_start(item: u32, call: &str) -> FixtureEventBody {
    FixtureEventBody::ToolStart {
        item,
        provider_call_id: call.into(),
        tool_name: "workspace_read".into(),
    }
}
fn tool(arguments: &[u8]) -> FixtureInput {
    input(vec![
        tool_start(0, "call"),
        FixtureEventBody::ArgumentsDelta {
            item: 0,
            bytes: arguments.to_vec(),
        },
        FixtureEventBody::ToolEnd { item: 0 },
        FixtureEventBody::Finish {
            reason: ModelFinish::Tools,
        },
    ])
}
fn usage() -> FixtureEventBody {
    FixtureEventBody::Usage {
        input_tokens: Counter::new("0").unwrap(),
        output_tokens: Counter::new("9223372036854775807").unwrap(),
    }
}

#[test]
fn catalog_cases_and_normalized_references() {
    let references: Value =
        serde_json::from_str(include_str!("../../../fixtures/models/responses.json")).unwrap();
    for reference in references["responses"].as_array().unwrap() {
        let message =
            decode_internal_message(&serde_json::to_vec(&reference["message"]).unwrap()).unwrap();
        let WireMessage::NormalizedModelResponse { payload, .. } = message else {
            panic!()
        };
        let name = match reference["name"].as_str().unwrap() {
            "complete_proposal" => "complete_split_arguments",
            "interrupted_missing_usage" => "partial_eof",
            name => name,
        };
        assert_eq!(result(&case(name)).response, *payload);
    }
    for (name, expected) in [
        ("changed_fragment_replay", FixtureError::ReplayMismatch),
        (
            "repeated_fragment_new_sequence",
            FixtureError::InvalidArguments,
        ),
        ("new_event_after_finish", FixtureError::InvalidFinish),
    ] {
        both_error(&case(name), expected);
    }
    let base = result(&case("complete_split_arguments"));
    assert!(result(&case("exact_fragment_replay")) == base);
    assert_eq!(base.proposal_identities.len(), 1);
    assert_eq!(
        base.proposal_identities[0].invocation_id,
        case("complete_split_arguments").seed.invocation_slots[0]
    );
    assert_eq!(base.proposal_identities[0].item, 0);
    assert!(base.usage == FixtureUsage::Unknown);
    assert!(base.response.usage_refs.as_slice().is_empty());
    let interrupted = result(&case("interrupted_after_closed_tool"));
    assert!(interrupted.response.items.as_slice().is_empty());
    assert!(interrupted.proposal_identities.is_empty());
    assert_eq!(interrupted.response.finish, ModelFinish::Interrupted);
    let zero = result(&case("usage_known_zero"));
    assert!(
        matches!(zero.usage, FixtureUsage::Known { input_tokens, output_tokens }
        if input_tokens.value() == 0 && output_tokens.value() == 0)
    );
    assert_eq!(
        zero.response.usage_refs.as_slice(),
        &[case("usage_known_zero").seed.usage_ref]
    );
    let failed = result(&case("failed"));
    let diagnostic = failed.response.diagnostic_error.0.unwrap();
    assert_eq!(diagnostic.message.as_str(), "Fixture model stream failed");
    assert_eq!(
        diagnostic.code,
        avencrew_contracts::wire::ErrorCode::Unavailable
    );
    assert!(diagnostic.evidence_refs.as_slice().is_empty());
    let utf8 = result(&case("split_utf8_text"));
    assert!(
        matches!(&utf8.response.items.as_slice()[0], Item::Text { text }
        if text.as_str() == "😀")
    );
    assert!(matches!(
        &result(&case("opaque_compatible")).response.items.as_slice()[0],
        Item::OpaqueContinuation { .. }
    ));
}

#[test]
fn interleaving_preserves_creation_and_host_slot_order() {
    use FixtureEventBody::*;
    let input = input(vec![
        tool_start(0, "first"),
        TextStart { item: 1 },
        tool_start(2, "second"),
        ArgumentsDelta {
            item: 2,
            bytes: b"{}".to_vec(),
        },
        ToolEnd { item: 2 },
        TextDelta {
            item: 1,
            bytes: b" ".to_vec(),
        },
        TextEnd { item: 1 },
        ArgumentsDelta {
            item: 0,
            bytes: b"{}".to_vec(),
        },
        ToolEnd { item: 0 },
        Finish {
            reason: ModelFinish::Tools,
        },
    ]);
    let output = result(&input);
    assert_eq!(
        output.proposal_identities[0].invocation_id,
        input.seed.invocation_slots[0]
    );
    assert_eq!(
        output.proposal_identities[1].invocation_id,
        input.seed.invocation_slots[1]
    );
    assert!(matches!(&output.response.items.as_slice()[0],
        Item::ToolProposal { provider_call_id, .. } if provider_call_id.as_str() == "first"));
    assert!(
        matches!(&output.response.items.as_slice()[1], Item::Text { text }
        if text.as_str() == " ")
    );
    assert!(result(&input) == output);
    let mut changed = input.clone();
    changed.seed.model_call_id = id(1000);
    changed.seed.step_id = id(1001);
    assert_eq!(result(&changed).response.model_call_id, id(1000));
    assert_eq!(result(&changed).response.step_id, id(1001));
}

#[test]
fn replay_and_sequence_fail_closed() {
    let mut fixture = case("candidate_only");
    let expected = result(&fixture);
    fixture.events.extend(fixture.events.clone());
    assert!(result(&fixture) == expected); // Replaying earlier events after Finish is legal.
    for sequence in ["0", "2", "9223372036854775807"] {
        let mut bad = case("candidate_only");
        bad.events[0].sequence = Counter::new(sequence).unwrap();
        both_error(&bad, FixtureError::InvalidSequence);
    }
    let mut bad = case("candidate_only");
    bad.events.swap(1, 2);
    both_error(&bad, FixtureError::InvalidSequence);
    let mut bad = case("exact_fragment_replay");
    bad.events[2].body = FixtureEventBody::TextEnd { item: 0 };
    both_error(&bad, FixtureError::ReplayMismatch);
}

#[test]
fn interruption_withholds_closed_tools_and_opaque_but_retains_closed_text() {
    use FixtureEventBody::*;
    let seed = case("candidate_only").seed;
    let mut bodies = vec![
        TextStart { item: 0 },
        TextDelta {
            item: 0,
            bytes: b"retained".to_vec(),
        },
        TextEnd { item: 0 },
        tool_start(1, "call"),
        ArgumentsDelta {
            item: 1,
            bytes: b"{}".to_vec(),
        },
        ToolEnd { item: 1 },
        Opaque {
            item: 2,
            provider_profile_id: seed.provider_profile_id,
            compatibility_digest: seed.compatibility_digest,
            blob_ref: seed.continuation_refs[0].clone(),
        },
        TextStart { item: 3 },
        TextDelta {
            item: 3,
            bytes: b"unfinished".to_vec(),
        },
        usage(),
    ];
    for finish in [
        None,
        Some(ModelFinish::Length),
        Some(ModelFinish::Interrupted),
        Some(ModelFinish::Failed),
    ] {
        let mut current = bodies.clone();
        if let Some(reason) = finish {
            current.push(Finish { reason });
        }
        let output = result(&input(current));
        assert_eq!(output.response.items.as_slice().len(), 1);
        assert!(
            matches!(&output.response.items.as_slice()[0], Item::Text { text }
            if text.as_str() == "retained")
        );
        assert!(output.proposal_identities.is_empty());
        assert!(matches!(output.usage, FixtureUsage::Known { .. }));
    }
    bodies.push(Finish {
        reason: ModelFinish::Tools,
    });
    both_error(&input(bodies), FixtureError::InvalidFinish);
    for reason in [ModelFinish::Final, ModelFinish::Refused] {
        let mut bad = tool(b"{}");
        bad.events.last_mut().unwrap().body = Finish { reason };
        both_error(&bad, FixtureError::InvalidFinish);
    }
    both_error(
        &input(vec![Finish {
            reason: ModelFinish::Tools,
        }]),
        FixtureError::InvalidFinish,
    );
    assert_eq!(
        result(&input(vec![])).response.finish,
        ModelFinish::Interrupted
    );
}

#[test]
fn invalid_item_transitions_bindings_calls_and_slots() {
    use FixtureEventBody::*;
    for bodies in [
        vec![TextStart { item: 1 }],
        vec![TextStart { item: 0 }, TextStart { item: 0 }],
        vec![TextDelta {
            item: 0,
            bytes: vec![],
        }],
        vec![ToolEnd { item: 0 }],
        vec![
            TextStart { item: 0 },
            ArgumentsDelta {
                item: 0,
                bytes: vec![],
            },
        ],
        vec![tool_start(0, "call"), TextEnd { item: 0 }],
        vec![
            TextStart { item: 0 },
            TextEnd { item: 0 },
            TextEnd { item: 0 },
        ],
        vec![
            TextStart { item: 0 },
            TextEnd { item: 0 },
            TextDelta {
                item: 0,
                bytes: vec![],
            },
        ],
    ] {
        both_error(&input(bodies), FixtureError::InvalidItemState);
    }
    let mut bad = tool(b"{}");
    bad.events[0].body = ToolStart {
        item: 0,
        provider_call_id: "call".into(),
        tool_name: "unknown".into(),
    };
    both_error(&bad, FixtureError::UnknownBinding);
    both_error(
        &input(vec![tool_start(0, "same"), tool_start(1, "same")]),
        FixtureError::DuplicateCall,
    );
    let mut bad = tool(b"{}");
    bad.seed.invocation_slots.clear();
    both_error(&bad, FixtureError::LimitExceeded);
    for call in ["".to_string(), "x".repeat(257), "é".repeat(129)] {
        let mut bad = tool(b"{}");
        bad.events[0].body = tool_start(0, &call);
        both_error(&bad, FixtureError::InvalidInput);
    }
    let mut valid = tool(b"{}");
    valid.events[0].body = tool_start(0, &"é".repeat(128));
    result(&valid);
}

#[test]
fn strict_arguments_utf8_and_fractional_numbers() {
    for bytes in [
        b"{".as_slice(),
        b"{} {}",
        b"[]",
        b"1",
        b"null",
        br#"{"a":1,"a":2}"#,
        br#"{"a":1,"\u0061":2}"#,
        br#"{"a":{"b":1,"b":2}}"#,
        br#"{"a":1e999}"#,
        br#"{}},{"kind":"text","text":"injected""#,
    ] {
        both_error(&tool(bytes), FixtureError::InvalidArguments);
    }
    both_error(
        &tool(&[b'{', b'"', 255, b'"', b':', b'0', b'}']),
        FixtureError::InvalidUtf8,
    );
    let fractional = result(&tool(br#"{"finite":1.25,"exponent":1e-2}"#));
    assert!(matches!(&fractional.response.items.as_slice()[0],
        Item::ToolProposal { arguments, .. } if arguments.as_map()["finite"] == json!(1.25)));
    let mut invalid = case("split_utf8_text");
    invalid.events[1].body = FixtureEventBody::TextDelta {
        item: 0,
        bytes: vec![255],
    };
    both_error(&invalid, FixtureError::InvalidUtf8);
    let mut incomplete = case("split_utf8_text");
    incomplete.events.truncate(2);
    assert!(result(&incomplete).response.items.as_slice().is_empty());
    let unicode = tool(r#"{"text":"😀"}"#.as_bytes());
    let FixtureEventBody::ArgumentsDelta { bytes, .. } = &unicode.events[1].body else {
        panic!()
    };
    let split = input(vec![
        tool_start(0, "call"),
        FixtureEventBody::ArgumentsDelta {
            item: 0,
            bytes: bytes[..11].to_vec(),
        },
        FixtureEventBody::ArgumentsDelta {
            item: 0,
            bytes: bytes[11..].to_vec(),
        },
        FixtureEventBody::ToolEnd { item: 0 },
        FixtureEventBody::Finish {
            reason: ModelFinish::Tools,
        },
    ]);
    assert!(result(&split) == result(&unicode));
}

#[test]
fn usage_retained_on_failure_and_never_read_after_finish() {
    let mut fixture = input(vec![usage(), FixtureEventBody::TextEnd { item: 0 }]);
    let failed = error(&fixture, FixtureError::InvalidItemState);
    assert!(
        matches!(failed.usage, FixtureUsage::Known { output_tokens, .. }
        if output_tokens.value() == i64::MAX as u64)
    );
    fixture.events[1].body = usage();
    assert!(matches!(
        error(&fixture, FixtureError::InvalidItemState).usage,
        FixtureUsage::Known { .. }
    ));
    let mut replay = case("usage_known_zero");
    replay.events.insert(1, replay.events[0].clone());
    assert!(result(&replay) == result(&case("usage_known_zero")));
    let failed = error(&case("new_event_after_finish"), FixtureError::InvalidFinish);
    assert!(failed.usage == FixtureUsage::Unknown);
    assert_eq!(failed.to_string(), "InvalidFinish");
    assert_eq!(format!("{failed:?}"), "InvalidFinish");
}

#[test]
fn opaque_compatibility_is_exact_and_items_are_closed() {
    for field in ["provider_profile_id", "compatibility_digest", "blob_ref"] {
        let fixture = case("opaque_compatible");
        let mut value = serde_json::to_value(&fixture).unwrap();
        value["events"][0]["body"][field] = if field == "compatibility_digest" {
            json!("0".repeat(64))
        } else {
            json!(id(999))
        };
        let bad = decode_fixture(&serde_json::to_vec(&value).unwrap()).unwrap();
        both_error(&bad, FixtureError::IncompatibleContinuation);
    }
    let mut bad = case("opaque_compatible");
    bad.events[1].body = FixtureEventBody::TextEnd { item: 0 };
    both_error(&bad, FixtureError::InvalidItemState);
}

#[test]
fn outer_json_is_strict_and_scalar_failures_are_private() {
    let fixture = case("candidate_only");
    let bytes = serde_json::to_vec(&fixture).unwrap();
    for raw in [
        b"{}".to_vec(),
        [bytes.as_slice(), b" {}"].concat(),
        [b"{\"schema_version\":\"hidden\",", &bytes[1..]].concat(),
        [b"{\"schema_\\u0076ersion\":\"hidden\",", &bytes[1..]].concat(),
    ] {
        let Err(failed) = decode_fixture(&raw) else {
            panic!()
        };
        assert!(failed.usage == FixtureUsage::Unknown);
        assert!(!failed.to_string().contains("hidden"));
    }
    for (pointer, value) in [
        ("/extra", json!(true)),
        ("/seed/extra", json!(true)),
        ("/events/0/extra", json!(true)),
        ("/events/0/body/extra", json!(true)),
        ("/seed/bindings/0/extra", json!(true)),
        ("/seed/model_call_id", json!("not-an-id")),
        ("/seed/compatibility_digest", json!("invalid")),
        ("/events/0/sequence", json!("9223372036854775808")),
        ("/events/0/body/item", json!(0.0)),
    ] {
        let mut value_fixture = serde_json::to_value(&fixture).unwrap();
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        value_fixture
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(key.into(), value);
        let Err(failed) = decode_fixture(&serde_json::to_vec(&value_fixture).unwrap()) else {
            panic!()
        };
        assert_eq!(failed.code, FixtureError::InvalidInput);
        assert!(failed.usage == FixtureUsage::Unknown);
    }
    let mut bad = fixture;
    bad.schema_version = "other".into();
    both_error(&bad, FixtureError::IncompatibleVersion);
}

#[test]
fn seed_uniqueness_names_and_list_boundaries() {
    for name in ["", "A", "_a", "a-", "é", &"a".repeat(65)] {
        let mut bad = case("candidate_only");
        bad.seed.bindings[0].name = name.into();
        both_error(&bad, FixtureError::InvalidInput);
    }
    let mut fixture = case("candidate_only");
    fixture.seed.bindings[0].name = format!("a{}", "_0.".repeat(21));
    result(&fixture); // Exactly 64 ASCII bytes.
    fixture.seed.bindings = (0..100)
        .map(|i| FixtureBinding {
            name: format!("tool_{i}"),
            binding_id: id(1000 + i),
        })
        .collect();
    fixture.seed.invocation_slots = (0..100).map(|i| id(2000 + i)).collect();
    fixture.seed.continuation_refs = (0..100).map(|i| id(3000 + i)).collect();
    result(&fixture);
    for field in 0..3 {
        let mut bad = fixture.clone();
        match field {
            0 => bad.seed.bindings.push(FixtureBinding {
                name: "extra".into(),
                binding_id: id(4000),
            }),
            1 => bad.seed.invocation_slots.push(id(4000)),
            _ => bad.seed.continuation_refs.push(id(4000)),
        }
        both_error(&bad, FixtureError::LimitExceeded);
    }
    for field in 0..4 {
        let mut bad = fixture.clone();
        match field {
            0 => bad.seed.bindings[1].name = bad.seed.bindings[0].name.clone(),
            1 => bad.seed.bindings[1].binding_id = bad.seed.bindings[0].binding_id.clone(),
            2 => bad.seed.invocation_slots[1] = bad.seed.invocation_slots[0].clone(),
            _ => bad.seed.continuation_refs[1] = bad.seed.continuation_refs[0].clone(),
        }
        both_error(&bad, FixtureError::InvalidInput);
    }
    for reserved in [
        fixture.seed.model_call_id.clone(),
        fixture.seed.step_id.clone(),
        fixture.seed.provider_profile_id.clone(),
        fixture.seed.usage_ref.clone(),
    ] {
        let mut bad = fixture.clone();
        bad.seed.invocation_slots[0] = reserved;
        both_error(&bad, FixtureError::InvalidInput);
    }
}

#[test]
fn event_and_item_count_boundaries() {
    let mut fixture = case("refusal");
    fixture.events = vec![fixture.events[0].clone(); 4096];
    result(&fixture);
    fixture.events.push(fixture.events[0].clone());
    both_error(&fixture, FixtureError::LimitExceeded);
    let mut bodies = Vec::new();
    for item in 0..100 {
        bodies.push(FixtureEventBody::TextStart { item });
        bodies.push(FixtureEventBody::TextEnd { item });
    }
    assert_eq!(
        result(&input(bodies.clone()))
            .response
            .items
            .as_slice()
            .len(),
        100
    );
    bodies.push(FixtureEventBody::TextStart { item: 100 });
    both_error(&input(bodies), FixtureError::LimitExceeded);
    both_error(
        &input(vec![FixtureEventBody::TextStart { item: u32::MAX }]),
        FixtureError::LimitExceeded,
    );
}

#[test]
fn text_argument_and_property_boundaries() {
    use FixtureEventBody::*;
    for size in [262144, 262145] {
        let fixture = input(vec![
            TextStart { item: 0 },
            TextDelta {
                item: 0,
                bytes: vec![b'a'; size],
            },
            TextEnd { item: 0 },
            Finish {
                reason: ModelFinish::Final,
            },
        ]);
        let arguments = format!("{{\"x\":\"{}\"}}", "a".repeat(size - 8));
        if size == 262144 {
            result(&fixture);
            result(&tool(arguments.as_bytes()));
        } else {
            both_error(&fixture, FixtureError::LimitExceeded);
            both_error(&tool(arguments.as_bytes()), FixtureError::LimitExceeded);
        }
    }
    for count in [128, 129] {
        let arguments = Value::Object((0..count).map(|i| (format!("k{i}"), json!(0))).collect());
        let fixture = tool(&serde_json::to_vec(&arguments).unwrap());
        if count == 128 {
            result(&fixture);
        } else {
            both_error(&fixture, FixtureError::InvalidArguments);
        }
    }
    // Raw cap cannot be bypassed by insignificant whitespace.
    let arguments = format!("{}{{}}", " ".repeat(262143));
    both_error(&tool(arguments.as_bytes()), FixtureError::LimitExceeded);
}

#[test]
fn raw_and_direct_encoded_input_caps_are_exact() {
    const MAX: usize = 1_048_576;
    let bytes = serde_json::to_vec(&case("candidate_only")).unwrap();
    let mut padded = bytes.clone();
    padded.resize(MAX, b' ');
    result(&decode_fixture(&padded).unwrap());
    padded.push(b' ');
    let Err(failed) = decode_fixture(&padded) else {
        panic!()
    };
    assert_eq!(failed.code, FixtureError::LimitExceeded);

    // Two unfinished text buffers remain below their individual bounds.
    // Tune decimal-byte encoding to exercise the direct counting sink exactly.
    let mut fixture = input(vec![
        FixtureEventBody::TextStart { item: 0 },
        FixtureEventBody::TextDelta {
            item: 0,
            bytes: vec![0; 262144],
        },
        FixtureEventBody::TextStart { item: 1 },
        FixtureEventBody::TextDelta {
            item: 1,
            bytes: vec![],
        },
    ]);
    let base = serde_json::to_vec(&fixture).unwrap().len();
    let needed = MAX - base;
    let FixtureEventBody::TextDelta { bytes, .. } = &mut fixture.events[3].body else {
        panic!()
    };
    // Replacing [] with [0,...] increases length by 2*n-1.
    bytes.resize(needed.div_ceil(2), 0);
    if needed.is_multiple_of(2) {
        bytes[0] = 10;
    }
    assert_eq!(serde_json::to_vec(&fixture).unwrap().len(), MAX);
    result(&fixture);
    decode_fixture(&serde_json::to_vec(&fixture).unwrap()).unwrap();
    let FixtureEventBody::TextDelta { bytes, .. } = &mut fixture.events[1].body else {
        panic!()
    };
    bytes[0] = 10;
    assert_eq!(serde_json::to_vec(&fixture).unwrap().len(), MAX + 1);
    both_error(&fixture, FixtureError::LimitExceeded);
}

#[test]
fn final_wire_cap_and_aggregate_delta_cap_cannot_be_bypassed() {
    use FixtureEventBody::*;
    // JSON escaping expands NUL text beyond the wire cap while input remains bounded.
    let fixture = input(vec![
        TextStart { item: 0 },
        TextDelta {
            item: 0,
            bytes: vec![0; 200000],
        },
        TextEnd { item: 0 },
        usage(),
        Finish {
            reason: ModelFinish::Final,
        },
    ]);
    let failed = error(&fixture, FixtureError::InvalidResponse);
    assert!(matches!(failed.usage, FixtureUsage::Known { .. }));
    let mut exact = input(vec![
        TextStart { item: 0 },
        TextDelta {
            item: 0,
            bytes: vec![],
        },
        TextEnd { item: 0 },
        Finish {
            reason: ModelFinish::Final,
        },
    ]);
    let empty = result(&exact).response;
    let base = avencrew_contracts::encode_message(&WireMessage::NormalizedModelResponse {
        schema_version: avencrew_contracts::wire::ProtocolVersion::V10,
        payload: Box::new(empty),
    })
    .unwrap()
    .len();
    let needed = 1_048_576 - base;
    let TextDelta { bytes, .. } = &mut exact.events[1].body else {
        panic!()
    };
    bytes.resize(needed / 6, 0);
    bytes.extend(vec![b'a'; needed % 6]);
    let exact_response = result(&exact).response;
    assert_eq!(
        avencrew_contracts::encode_message(&WireMessage::NormalizedModelResponse {
            schema_version: avencrew_contracts::wire::ProtocolVersion::V10,
            payload: Box::new(exact_response),
        })
        .unwrap()
        .len(),
        1_048_576
    );
    let TextDelta { bytes, .. } = &mut exact.events[1].body else {
        panic!()
    };
    bytes.push(b'a');
    both_error(&exact, FixtureError::InvalidResponse);
    let mut fixture = input(vec![TextDelta {
        item: 0,
        bytes: vec![0; 262144],
    }]);
    fixture.events = vec![fixture.events[0].clone(); 5];
    both_error(&fixture, FixtureError::LimitExceeded);
}
