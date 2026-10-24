//! Broker host-port conformance; shared synthetic host remains in-memory.
mod support;
use avencrew_broker::*;
use avencrew_contracts::scalars::{Digest, JsonObject};
use avencrew_harness::model::fixture::commit_fixture_proposals;
use serde_json::{json, Value};
use support::*;
async fn committed(broker: &Broker, host: &mut FixtureHost, request: &Invocation) {
    broker.commit(host, vec![request.clone()]).await.unwrap();
}
async fn assert_error(
    broker: &Broker,
    host: &mut FixtureHost,
    request: Invocation,
    code: BrokerError,
) {
    let Err(error) = broker.invoke(host, request).await else {
        panic!("unexpected broker success")
    };
    assert_eq!(error, code);
    assert_eq!(host.dispatch_count, 0);
}

#[test]
fn complete_fixture_to_broker_replay_and_partial_stream_withholding() {
    run(async {
        let (broker, mut host) = setup();
        let (input, result) = fixture("complete_split_arguments");
        let proposals =
            commit_fixture_proposals(&broker, &mut host, &input.seed.model_call_id, &result)
                .await
                .unwrap();
        assert_eq!(proposals.len(), 1);
        let request = Invocation::from_model(
            proposals[0].invocation_id().clone(),
            input.seed.model_call_id.clone(),
            &result.response.items.as_slice()[0],
        )
        .unwrap();
        assert!(matches!(
            broker.invoke(&mut host, request.clone()).await.unwrap(),
            Outcome::Result(_)
        ));
        let (_, replayed) = fixture("exact_fragment_replay");
        commit_fixture_proposals(&broker, &mut host, &input.seed.model_call_id, &replayed)
            .await
            .unwrap();
        assert!(matches!(
            broker.invoke(&mut host, request).await.unwrap(),
            Outcome::Result(_)
        ));
        assert_eq!(host.dispatch_count, 1);
        for name in [
            "partial_eof",
            "interrupted_after_closed_tool",
            "candidate_only",
            "failed",
        ] {
            let (_, result) = fixture(name);
            let before = host.commit_count;
            assert!(commit_fixture_proposals(
                &broker,
                &mut host,
                &input.seed.model_call_id,
                &result
            )
            .await
            .is_err());
            assert_eq!(host.commit_count, before);
            assert_eq!(host.dispatch_count, 1);
        }
    });
}

#[test]
fn origin_matrix_shares_schema_commit_permission_lease_and_result_gates() {
    run(async {
        for origin in [
            Origin::Model {
                model_call_id: id(16),
                provider_call_id: "call".into(),
            },
            Origin::Supervisor,
            Origin::Ptc {
                parent_invocation_id: id(900),
            },
        ] {
            let (broker, mut host) = setup();
            if origin == Origin::Supervisor {
                host.policy.model_call_id = None;
            }
            let request = invocation(600, origin);
            assert_error(
                &broker,
                &mut host,
                request.clone(),
                BrokerError::NotCommitted,
            )
            .await;
            committed(&broker, &mut host, &request).await;
            host.policy.target_allowed = false;
            assert_error(&broker, &mut host, request.clone(), BrokerError::Denied).await;
            host.policy.target_allowed = true;
            host.policy.lease.epoch = serde_json::from_value(json!("2")).unwrap();
            assert_error(
                &broker,
                &mut host,
                request.clone(),
                BrokerError::StaleAuthority,
            )
            .await;
            host.policy.lease.epoch = serde_json::from_value(json!("1")).unwrap();
            let mut bad = request.clone();
            bad.arguments =
                serde_json::from_value(json!({"source_name":"account_snapshot","epoch":"9"}))
                    .unwrap();
            assert_error(&broker, &mut host, bad, BrokerError::InvalidArguments).await;
            assert!(matches!(
                broker.invoke(&mut host, request.clone()).await.unwrap(),
                Outcome::Result(_)
            ));
            assert_eq!(host.dispatch_count, 1);
            host.policy.max_result_bytes = 1;
            assert!(matches!(
                broker.invoke(&mut host, request).await,
                Err(BrokerError::LimitExceeded)
            ));
            assert_eq!(host.dispatch_count, 1);
        }
    });
}

#[test]
fn every_live_authority_layer_denies_before_execution() {
    run(async {
        type PolicyMutation = (fn(&mut PolicySnapshot), BrokerError);
        let mutations: Vec<PolicyMutation> = vec![
            (|p| p.principal_active = false, BrokerError::Denied),
            (|p| p.workspace_allowed = false, BrokerError::Denied),
            (|p| p.project_allowed = false, BrokerError::Denied),
            (|p| p.task_allowed = false, BrokerError::Denied),
            (|p| p.worker_allowed = false, BrokerError::Denied),
            (|p| p.installation_allowed = false, BrokerError::Denied),
            (|p| p.environment_allowed = false, BrokerError::Denied),
            (|p| p.target_allowed = false, BrokerError::Denied),
            (|p| p.budget_reserved = false, BrokerError::Denied),
            (|p| p.dispatch_supported = false, BrokerError::Unsupported),
            (|p| p.cancellation_requested = true, BrokerError::Cancelled),
            (|p| p.unresolved_effect = true, BrokerError::UnknownOutcome),
            (
                |p| p.context.workspace_id = id(999),
                BrokerError::StaleAuthority,
            ),
            (
                |p| p.context.project_id.0 = Some(id(999)),
                BrokerError::StaleAuthority,
            ),
            (
                |p| p.context.principal_id = id(999),
                BrokerError::StaleAuthority,
            ),
            (|p| p.context.run_id = id(999), BrokerError::StaleAuthority),
            (
                |p| p.context.attempt_id = id(999),
                BrokerError::StaleAuthority,
            ),
            (|p| p.context.step_id = id(999), BrokerError::StaleAuthority),
            (
                |p| p.context.catalog_digest = digest(),
                BrokerError::StaleAuthority,
            ),
            (
                |p| p.lease.authority = avencrew_contracts::wire::ExecutionAuthority::Server,
                BrokerError::StaleAuthority,
            ),
            (
                |p| p.lease.placement = avencrew_contracts::wire::Placement::Cloud,
                BrokerError::StaleAuthority,
            ),
            (
                |p| p.lease.epoch = serde_json::from_value(json!("2")).unwrap(),
                BrokerError::StaleAuthority,
            ),
            (|p| p.lease.boot_id = id(999), BrokerError::StaleAuthority),
            (|p| p.boot_id = id(999), BrokerError::StaleAuthority),
            (
                |p| p.lease.generation.0 = Some(serde_json::from_value(json!("2")).unwrap()),
                BrokerError::StaleAuthority,
            ),
            (
                |p| p.lease.expires_at = p.now.clone(),
                BrokerError::StaleAuthority,
            ),
            (
                |p| p.monotonic_now_ns = p.lease.monotonic_deadline_ns.clone(),
                BrokerError::StaleAuthority,
            ),
            (
                |p| p.permission_generation = serde_json::from_value(json!("2")).unwrap(),
                BrokerError::StaleAuthority,
            ),
            (
                |p| p.deletion_generation = serde_json::from_value(json!("1")).unwrap(),
                BrokerError::StaleAuthority,
            ),
            (
                |p| p.model_call_id = Some(id(999)),
                BrokerError::InvalidOrigin,
            ),
            (|p| p.max_result_bytes = 262145, BrokerError::LimitExceeded),
        ];
        for (change, expected) in mutations {
            let (broker, mut host) = setup();
            let request = model(600);
            committed(&broker, &mut host, &request).await;
            change(&mut host.policy);
            assert_error(&broker, &mut host, request, expected).await;
        }
    });
}

#[test]
fn policy_race_and_output_revocation_do_not_release_stale_data() {
    run(async {
        let (broker, mut host) = setup();
        let request = model(600);
        committed(&broker, &mut host, &request).await;
        host.revoke_at_claim = true;
        assert_error(
            &broker,
            &mut host,
            request.clone(),
            BrokerError::StaleAuthority,
        )
        .await;
        assert!(matches!(host.rows[&id(600)].1, Retained::Proposed));
        host.revoke_at_claim = false;
        host.policy.target_allowed = true;
        host.revoke_at_release = true;
        let Err(error) = broker.invoke(&mut host, request.clone()).await else {
            panic!()
        };
        assert_eq!(error, BrokerError::Denied);
        assert!(matches!(host.rows[&id(600)].1, Retained::Result(_)));
        assert_eq!(host.dispatch_count, 1);
        host.revoke_at_release = false;
        host.policy.target_allowed = true;
        assert!(matches!(
            broker.invoke(&mut host, request).await.unwrap(),
            Outcome::Result(_)
        ));
        assert_eq!(host.dispatch_count, 1);
    });
}

#[test]
fn changed_identity_provider_mapping_and_failed_batch_never_dispatch() {
    run(async {
        let (broker, mut host) = setup();
        let request = model(600);
        committed(&broker, &mut host, &request).await;
        let mut changed = request.clone();
        changed.arguments =
            serde_json::from_value(json!({"source_name":"private_renewal_note"})).unwrap();
        assert_error(
            &broker,
            &mut host,
            changed.clone(),
            BrokerError::IdempotencyMismatch,
        )
        .await;
        assert!(broker
            .commit(&mut host, vec![model(601), changed])
            .await
            .is_err());
        assert!(!host.rows.contains_key(&id(601)));
        let mut duplicate_call = model(601);
        duplicate_call.origin = request.origin.clone();
        assert!(broker
            .commit(&mut host, vec![duplicate_call])
            .await
            .is_err());
        let mut bad = model(602);
        bad.binding_id = id(999);
        assert!(broker
            .commit(&mut host, vec![model(601), bad])
            .await
            .is_err());
        assert!(!host.rows.contains_key(&id(601)));
        host.fail_commit = true;
        assert!(broker.commit(&mut host, vec![model(601)]).await.is_err());
        assert!(!host.rows.contains_key(&id(601)));
        host.fail_commit = false;
        host.fail_claim = true;
        assert_error(&broker, &mut host, request, BrokerError::Storage).await;
    });
}

#[test]
fn dispatch_uncertainty_and_lost_result_commit_do_not_repeat_execution() {
    run(async {
        for fail_result in [false, true] {
            let (broker, mut host) = setup();
            let request = model(600);
            committed(&broker, &mut host, &request).await;
            host.fail_execute = !fail_result;
            host.fail_result_commit = fail_result;
            let first = broker.invoke(&mut host, request.clone()).await;
            if fail_result {
                assert!(matches!(first, Err(BrokerError::UnknownOutcome)));
            } else {
                assert!(matches!(first, Ok(Outcome::Unknown)));
            }
            host.fail_execute = false;
            host.fail_result_commit = false;
            assert!(matches!(
                broker.invoke(&mut host, request).await.unwrap(),
                Outcome::Unknown | Outcome::Pending
            ));
            assert_eq!(host.dispatch_count, 1);
        }
    });
}

#[test]
fn pinned_catalog_digest_order_pagination_and_invalid_schemas() {
    let original = binding(18, "workspace_read");
    let extra = binding(19, "other_read");
    let catalog = SealedCatalog::seal(vec![original.clone(), extra.clone()]).unwrap();
    let reversed = SealedCatalog::seal(vec![extra.clone(), original.clone()]).unwrap();
    assert_eq!(catalog.digest(), reversed.digest());
    let first = catalog.search("", 0, 1).unwrap();
    assert_eq!(first.bindings[0].binding_id, id(18));
    let next = catalog.search("", first.next_offset.unwrap(), 1).unwrap();
    assert_eq!(next.bindings[0].binding_id, id(19));
    assert!(next.next_offset.is_none());
    assert_eq!(
        catalog.search("workspace", 0, 100).unwrap().bindings.len(),
        1
    );
    for (offset, limit) in [(0, 0), (0, 101), (3, 1)] {
        assert!(catalog.search("", offset, limit).is_err());
    }
    let mut changed = original.clone();
    changed.adapter_digest = Digest::new("1".repeat(64)).unwrap();
    assert_ne!(
        catalog.digest(),
        SealedCatalog::seal(vec![changed, extra]).unwrap().digest()
    );
    assert_eq!(
        catalog.bindings()[0].adapter_digest,
        original.adapter_digest
    );
    for schema in [
        json!({"type":"object"}),
        json!({"type":"array"}),
        json!({"type":"object","additionalProperties":false,"$ref":"https://secret.invalid"}),
        json!({"type":"object","additionalProperties":false,
            "properties":{"q":{"type":"string","pattern":"(a+)+$"}}}),
    ] {
        let mut invalid = original.clone();
        invalid.input_schema = schema;
        assert!(SealedCatalog::seal(vec![invalid]).is_err());
    }
    assert!(SealedCatalog::seal(vec![original.clone(), original]).is_err());
}

#[test]
fn origin_provenance_parent_and_effect_restrictions() {
    run(async {
        let (broker, mut host) = setup();
        let supervisor = invocation(600, Origin::Supervisor);
        committed(&broker, &mut host, &supervisor).await;
        assert_error(&broker, &mut host, supervisor, BrokerError::InvalidOrigin).await;
        let child = invocation(
            601,
            Origin::Ptc {
                parent_invocation_id: id(900),
            },
        );
        committed(&broker, &mut host, &child).await;
        host.policy.parent_active_same_step = false;
        assert_error(&broker, &mut host, child, BrokerError::Denied).await;
        let self_parent = invocation(
            602,
            Origin::Ptc {
                parent_invocation_id: id(602),
            },
        );
        assert_error(&broker, &mut host, self_parent, BrokerError::InvalidOrigin).await;
        for effect in [
            Effect::IsolatedMutation,
            Effect::SharedMutation,
            Effect::ExternalEffect,
            Effect::PrivilegedExecution,
        ] {
            let mut binding = binding(18, "workspace_read");
            binding.effect = effect;
            let (broker, mut host) = setup_with(vec![binding]);
            let direct = model(600);
            committed(&broker, &mut host, &direct).await;
            assert_error(&broker, &mut host, direct, BrokerError::Unsupported).await;
            assert_error(
                &broker,
                &mut host,
                invocation(
                    601,
                    Origin::Ptc {
                        parent_invocation_id: id(900),
                    },
                ),
                BrokerError::Denied,
            )
            .await;
        }
    });
}

#[test]
fn result_identity_schema_and_size_failures_retain_unknown() {
    run(async {
        for change in 0..4 {
            let (broker, mut host) = setup();
            let request = model(600);
            let prepared = broker.prepare(request.clone()).unwrap();
            let mut value = serde_json::to_value(host.success(&prepared)).unwrap();
            match change {
                0 => value["invocation_id"] = json!(id(999)),
                1 => value["data"]["binding_id"] = json!(id(999)),
                2 => value["data"]["value"]["unexpected"] = json!(true),
                _ => value["data"]["value"]["source_name"] = json!("a".repeat(32768)),
            }
            host.result_override = Some(serde_json::from_value(value).unwrap());
            committed(&broker, &mut host, &request).await;
            assert!(matches!(
                broker.invoke(&mut host, request.clone()).await.unwrap(),
                Outcome::Unknown
            ));
            assert!(matches!(
                broker.invoke(&mut host, request).await.unwrap(),
                Outcome::Unknown
            ));
            assert_eq!(host.dispatch_count, 1);
        }
    });
}

#[test]
fn fixture_mapping_tampering_fails_before_commit() {
    run(async {
        let (broker, mut host) = setup();
        let (input, mut output) = fixture("complete_split_arguments");
        output.proposal_identities[0].item = 99;
        assert!(
            commit_fixture_proposals(&broker, &mut host, &input.seed.model_call_id, &output)
                .await
                .is_err()
        );
        assert!(host.rows.is_empty());
        let (_, output) = fixture("complete_split_arguments");
        assert!(
            commit_fixture_proposals(&broker, &mut host, &id(999), &output)
                .await
                .is_err()
        );
        assert!(host.rows.is_empty());
    });
}

#[test]
fn supported_input_counts_bytes_and_scalar_numbers_are_bounded() {
    let mut binding = binding(18, "workspace_read");
    binding.input_schema = json!({"type":"object","additionalProperties":false,
        "properties":{"text":{"type":"string"},"number":{"type":"number"}}});
    let (broker, _) = setup_with(vec![binding]);
    for size in [262144, 262145] {
        let mut request = model(600);
        request.arguments = serde_json::from_value(json!({"text":"a".repeat(size-11)})).unwrap();
        let prepared = broker.prepare(request);
        if size == 262144 {
            assert!(prepared.is_ok());
        } else {
            assert!(matches!(prepared, Err(BrokerError::LimitExceeded)));
        }
    }
    let mut fractional = model(600);
    fractional.arguments = serde_json::from_value(json!({"number":1.25})).unwrap();
    broker.prepare(fractional).unwrap();
    let mut nested = Value::Null;
    for _ in 0..66 {
        nested = json!([nested]);
    }
    let mut request = model(600);
    request.arguments =
        JsonObject::new(serde_json::Map::from_iter([("text".into(), nested)])).unwrap();
    assert!(matches!(
        broker.prepare(request),
        Err(BrokerError::LimitExceeded)
    ));
    let mut bindings = Vec::new();
    for n in 0..100 {
        bindings.push(binding_for_count(n));
    }
    assert!(SealedCatalog::seal(bindings.clone()).is_ok());
    bindings.push(binding_for_count(100));
    assert!(matches!(
        SealedCatalog::seal(bindings),
        Err(BrokerError::LimitExceeded)
    ));
}
fn binding_for_count(n: usize) -> Binding {
    binding(2000 + n, &format!("read_{n}"))
}

#[test]
fn fresh_claim_clock_and_ambiguous_commit_are_fenced() {
    run(async {
        for clock in [20, 100000] {
            let (broker, mut host) = setup();
            let request = model(600);
            committed(&broker, &mut host, &request).await;
            host.claim_clock = Some(clock);
            if clock == 20 {
                assert!(matches!(
                    broker.invoke(&mut host, request).await.unwrap(),
                    Outcome::Result(_)
                ));
            } else {
                assert_error(&broker, &mut host, request, BrokerError::StaleAuthority).await;
            }
        }
        let (broker, mut host) = setup();
        let request = model(600);
        committed(&broker, &mut host, &request).await;
        host.fail_claim_reply = true;
        assert_error(
            &broker,
            &mut host,
            request.clone(),
            BrokerError::UnknownOutcome,
        )
        .await;
        host.fail_claim_reply = false;
        assert!(matches!(
            broker.invoke(&mut host, request).await.unwrap(),
            Outcome::Pending
        ));
        assert_eq!(host.dispatch_count, 0);
    });
}

#[test]
fn source_audience_tenant_and_replay_output_limits_are_enforced() {
    run(async {
        let (broker, mut host) = setup();
        let mut other = model(600);
        other.arguments =
            serde_json::from_value(json!({"source_name":"other_tenant_report"})).unwrap();
        committed(&broker, &mut host, &other).await;
        assert_error(&broker, &mut host, other, BrokerError::Denied).await;

        // Principal 6 may read public project reports, but not the private note.
        let (original, mut host) = setup();
        host.policy.context.principal_id = id(6);
        let broker = Broker::new(
            original.step().clone(),
            host.policy.context.clone(),
            host.policy.lease.clone(),
            original.catalog().clone(),
        )
        .unwrap();
        let mut private = model(601);
        private.arguments =
            serde_json::from_value(json!({"source_name":"private_renewal_note"})).unwrap();
        committed(&broker, &mut host, &private).await;
        assert_error(&broker, &mut host, private, BrokerError::Denied).await;
        let mut public = model(602);
        public.arguments =
            serde_json::from_value(json!({"source_name":"customer_export_report"})).unwrap();
        committed(&broker, &mut host, &public).await;
        let prepared = broker.prepare(public.clone()).unwrap();
        let bytes = serde_json::to_vec(&host.success(&prepared)).unwrap().len();
        host.policy.max_result_bytes = bytes as u32;
        assert!(matches!(
            broker.invoke(&mut host, public.clone()).await.unwrap(),
            Outcome::Result(_)
        ));
        host.policy.max_result_bytes -= 1;
        assert!(matches!(
            broker.invoke(&mut host, public).await,
            Err(BrokerError::LimitExceeded)
        ));
        assert_eq!(host.dispatch_count, 1);
        assert!(matches!(host.rows[&id(602)].1, Retained::Result(_)));
    });
}

#[test]
fn changing_registry_requires_a_new_committed_step() {
    run(async {
        let (original, mut host) = setup();
        let replacement = SealedCatalog::seal(vec![
            binding(18, "workspace_read"),
            binding(19, "other_read"),
        ])
        .unwrap();
        let mut step = original.step().clone();
        step.catalog_digest = replacement.digest().clone();
        step.tool_binding_ids = avencrew_contracts::scalars::List::new(
            replacement
                .bindings()
                .iter()
                .map(|b| b.binding_id.clone())
                .collect(),
        )
        .unwrap();
        let mut context = host.policy.context.clone();
        context.catalog_digest = replacement.digest().clone();
        let changed = Broker::new(
            step.clone(),
            context.clone(),
            host.policy.lease.clone(),
            replacement.clone(),
        )
        .unwrap();
        assert!(matches!(
            changed.commit(&mut host, vec![model(600)]).await,
            Err(BrokerError::InvalidCatalog)
        ));
        assert!(host.rows.is_empty());
        let mut new_binding = model(601);
        new_binding.binding_id = id(19);
        assert!(matches!(
            original.prepare(new_binding.clone()),
            Err(BrokerError::UnsealedBinding)
        ));

        // Trusted coordinator admits a different step and model call.
        step.step_id = id(700);
        context.step_id = step.step_id.clone();
        host.steps.insert(step.step_id.clone(), step.clone());
        host.policy.context = context.clone();
        host.policy.model_call_id = Some(id(701));
        let next = Broker::new(step, context, host.policy.lease.clone(), replacement).unwrap();
        new_binding.origin = Origin::Model {
            model_call_id: id(701),
            provider_call_id: "next".into(),
        };
        committed(&next, &mut host, &new_binding).await;
        assert!(matches!(
            next.invoke(&mut host, new_binding).await.unwrap(),
            Outcome::Result(_)
        ));
        assert_eq!(host.dispatch_count, 1);
        assert_eq!(original.catalog().bindings().len(), 1);
    });
}
