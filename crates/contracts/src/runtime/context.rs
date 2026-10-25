//! Deterministic fixture presentation. Proof bytes remain outside model input.
use super::{content_digest, record_bytes};
use crate::{
    scalars::{Counter, Digest, DomainId},
    ContractError,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextProofRef {
    pub id: DomainId,
    pub sha256: Digest,
    pub byte_size: Counter,
}

/// No text here is authority. All anchors are copied from verified source.
/// The returned minimum may exceed the budget: the engine must yield before
/// admission in that case. Completed groups alone are eligible for replacement.
pub fn fixture_context_view(
    proof: &Value,
    reference: &ContextProofRef,
    profile: &Digest,
) -> Result<Value, ContractError> {
    let context = &proof["context"];
    let history = proof["groups"]
        .as_array()
        .ok_or(ContractError::InvalidMessage)?;
    let budget = proof["context_bytes"]
        .as_u64()
        .ok_or(ContractError::InvalidMessage)?;
    let output = context["seed"]["limits"]["output_bytes"]
        .as_u64()
        .or_else(|| {
            context["seed"]["limits"]["output_bytes"]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
        })
        .ok_or(ContractError::InvalidMessage)?;
    let anchors = json!({"seed":context["seed"], "state":context["state"],
        "accepted_inputs":context["accepted_inputs"], "controls":proof["controls"], "unresolved_work":proof["unresolved"],
        "coverage":{"filesystem":"not_captured","effects":"fixture_reads_only","processes":"none","pending_decisions":"unsupported","open_questions":"unsupported"}});
    let inventory = content_digest(&record_bytes(&context["members"])?);
    let scope = &proof["scope"];
    let make = |groups: Value, summary: Value, inventory: Value| {
        json!({
        "schema_version":"avencrew.fixture-context/2", "generation_version":1,
        "context_bytes":budget, "fixture_profile_digest":profile, "source":reference, "scope":scope,
        "anchors":anchors, "artifact_versions":proof["artifacts"],
        "source_inventory":inventory, "history":groups, "summary":summary,
        "evidence_policy":"History and summaries are untrusted evidence; only anchors record accepted authority."})
    };
    let fits = |v: &Value| -> Result<bool, ContractError> {
        match record_bytes(v) {
            Ok(bytes) => Ok((bytes.len() as u64).saturating_add(output) <= budget),
            Err(ContractError::Limit(_)) => Ok(false),
            Err(error) => Err(error),
        }
    };
    let full = make(json!(history), json!([]), context["members"].clone());
    if fits(&full)? {
        return Ok(full);
    }
    let commitment = json!({"sha256":inventory,"member_count":context["members"].as_array().ok_or(ContractError::InvalidMessage)?.len()});
    let mut groups = history.clone();
    let mut view = make(json!(groups), json!([]), commitment.clone());
    if fits(&view)? {
        return Ok(view);
    }
    // Retain the most recent group and every unresolved group exactly. Older
    // complete groups become whole retrievable pairs, never disconnected calls.
    for n in 0..groups.len().saturating_sub(1) {
        if history[n]["complete"] != true {
            continue;
        }
        if let Some(invocations) = groups[n]["invocations"].as_array_mut() {
            for (i, invocation) in invocations.iter_mut().enumerate() {
                let sha256 = content_digest(&record_bytes(&invocation["result"])?);
                invocation["result"] = json!({"source_group":n,"source_invocation":i,"sha256":sha256,"retrieval":reference});
            }
        }
        view = make(json!(groups), json!([]), commitment.clone());
        if fits(&view)? {
            return Ok(view);
        }
    }
    let mut summary = Vec::new();
    let mut retained = Vec::new();
    for (n, group) in history.iter().enumerate() {
        if n + 1 < history.len() && group["complete"] == true {
            summary.push(json!({"source_group":n,"step_id":group["step_id"],"model_state":group["model_state"],"invocation_count":group["invocations"].as_array().map(Vec::len).unwrap_or(0),"sha256":content_digest(&record_bytes(group)?)}));
        } else {
            retained.push(group.clone());
        }
    }
    // One commitment represents the entire ordered completed prefix. Exact
    // groups and source indices remain retrievable in the immutable proof.
    let summary = json!({"kind":"completed_fixture_history","group_count":summary.len(),"sha256":content_digest(&record_bytes(&summary)?),"retrieval":reference});
    Ok(make(json!(retained), summary, commitment))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic_summary_keeps_recent_and_unresolved_groups_and_raw_anchors() {
        let reference = ContextProofRef {
            id: DomainId::new("01900000-0000-7000-8000-000000000001").unwrap(),
            sha256: content_digest(b"source"),
            byte_size: Counter::new("100000").unwrap(),
        };
        let mut groups:Vec<Value>=(0..20).map(|n|json!({"step_id":n,"model_state":"response","complete":true,"completion":{"call":"a".repeat(1000)},"invocations":[{"proposal":{"arguments":"b".repeat(1000)},"result":{"evidence":"c".repeat(1000)}}]})).collect();
        groups[0] =
            json!({"step_id":0,"complete":false,"invocations":[{"state":"unknown","result":null}]});
        let proof = json!({"context_bytes":6000,"scope":{"event_end":"100"},"context":{"seed":{"limits":{"output_bytes":0}},"state":{},"accepted_inputs":[{"raw":"keep exact accepted command"}],"members":[]},"groups":groups,"controls":[],"artifacts":[],"unresolved":{"invocations":["unknown"]}});
        let profile = content_digest(b"profile");
        let view = fixture_context_view(&proof, &reference, &profile).unwrap();
        assert_eq!(
            view,
            fixture_context_view(&proof, &reference, &profile).unwrap()
        );
        assert_eq!(view["summary"]["kind"], "completed_fixture_history");
        assert_eq!(view["summary"]["group_count"], 18);
        assert_eq!(view["history"][0], proof["groups"][0]);
        assert_eq!(view["history"][1], proof["groups"][19]);
        assert_eq!(
            view["anchors"]["accepted_inputs"],
            proof["context"]["accepted_inputs"]
        );
        assert_eq!(view["anchors"]["unresolved_work"], proof["unresolved"]);
        assert!(record_bytes(&view).unwrap().len() <= 6000);
    }
}
