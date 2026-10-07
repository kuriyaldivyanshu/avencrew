use avencrew_contracts::bundles::{BlobReference, BundleCandidate, BundleError, Purpose};
use avencrew_contracts::scalars::{Counter, Digest, DomainId};
use serde_json::{json, Value};

const WORKSPACE: &str = "01900000-0000-7000-8000-000000000001";
const ID: &str = "01900000-0000-7000-8000-000000000002";
fn workspace() -> DomainId {
    DomainId::new(WORKSPACE).unwrap()
}
fn input() -> Value {
    json!({"schema_version":"1.0","purpose":"task","root":{"table":"tasks","id":ID},
        "records":[{"table":"tasks","id":ID,"record_schema":"server-v1:S0",
            "record":{"id":ID,"workspace_id":WORKSPACE}}],"referenced_blobs":[]})
}
fn inspect(value: &Value) -> Result<BundleCandidate, BundleError> {
    BundleCandidate::inspect(&serde_json::to_vec(value).unwrap(), &workspace())
}

#[test]
fn canonical_round_trip_and_digest_are_stable() {
    let a = inspect(&input()).unwrap();
    let b = BundleCandidate::inspect(&serde_json::to_vec_pretty(&input()).unwrap(), &workspace())
        .unwrap();
    assert_eq!(a.purpose(), Purpose::Task);
    assert_eq!(a.canonical_bytes(), b.canonical_bytes());
    assert_eq!(a.sha256(), b.sha256());
    a.verify_digest(&b.sha256()).unwrap();
    assert_eq!(a.verify_digest(&[0; 32]), Err(BundleError::BlobMismatch));
    assert_eq!(
        inspect(&serde_json::from_slice(a.canonical_bytes()).unwrap())
            .unwrap()
            .sha256(),
        a.sha256()
    );
}

#[test]
fn hostile_envelopes_fail_without_publication() {
    let cases: &[(&str, Value, BundleError)] = &[
        ("version", json!("2.0"), BundleError::Version),
        ("extra", json!(true), BundleError::Shape),
    ];
    for (key, value, expected) in cases {
        let mut v = input();
        v[if *key == "version" {
            "schema_version"
        } else {
            key
        }] = value.clone();
        assert!(matches!(inspect(&v), Err(e) if e == *expected));
    }
    let mut v = input();
    v["root"]["id"] = json!(WORKSPACE);
    assert!(matches!(inspect(&v), Err(BundleError::MissingRoot)));
    let mut v = input();
    v["records"][0]["record"]["id"] = json!(WORKSPACE);
    assert!(matches!(inspect(&v), Err(BundleError::Identity)));
    let mut v = input();
    v["records"][0]["record"]["workspace_id"] = json!(ID);
    assert!(matches!(inspect(&v), Err(BundleError::Workspace)));
    let mut v = input();
    v["records"][0]["table"] = json!("resource_grants");
    assert!(matches!(inspect(&v), Err(BundleError::Purpose)));
    let mut v = input();
    v["records"][0]["record_schema"] = json!("server-v1:S99");
    assert!(matches!(inspect(&v), Err(BundleError::Purpose)));
    let mut v = input();
    v["purpose"] = json!("handoff");
    assert!(matches!(inspect(&v), Err(BundleError::Shape)));
    let mut v = input();
    v["records"] = json!([]);
    assert!(matches!(inspect(&v), Err(BundleError::Limit)));
    let mut v = input();
    v["root"]["table"] = json!("principals");
    assert!(matches!(inspect(&v), Err(BundleError::Purpose)));
}

#[test]
fn duplicates_are_consistent_or_rejected() {
    let mut v = input();
    let duplicate = v["records"][0].clone();
    v["records"].as_array_mut().unwrap().push(duplicate);
    inspect(&v).unwrap();
    v["records"][1]["record"]["extra"] = json!("different");
    assert!(matches!(inspect(&v), Err(BundleError::Duplicate)));
    let mut v = input();
    let reference = json!({"id":ID,"sha256":"0".repeat(64),"byte_size":"0"});
    v["referenced_blobs"] = json!([reference.clone(), reference]);
    inspect(&v).unwrap();
    v["referenced_blobs"][1]["byte_size"] = json!("1");
    assert!(matches!(inspect(&v), Err(BundleError::Duplicate)));
}

#[test]
fn raw_content_uses_known_sha256_and_exact_size() {
    let reference = BlobReference {
        id: DomainId::new(ID).unwrap(),
        sha256: Digest::new("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
            .unwrap(),
        byte_size: Counter::new("3").unwrap(),
    };
    reference.verify(b"abc").unwrap();
    assert_eq!(reference.verify(b"abd"), Err(BundleError::BlobMismatch));
    assert_eq!(reference.verify(b"ab"), Err(BundleError::BlobMismatch));
}

#[test]
fn strict_json_and_existing_bounds_are_not_bypassed() {
    assert!(BundleCandidate::inspect(br#"{"x":1,"x":2}"#, &workspace()).is_err());
    assert!(BundleCandidate::inspect(&vec![b' '; 1_048_577], &workspace()).is_err());
    let mut v = input();
    v["records"][0]["record"]["number"] = json!(1.5);
    assert!(matches!(inspect(&v), Err(BundleError::Json(_))));
    let mut v = input();
    v["referenced_blobs"] = json!([{"id":ID,"sha256":"0".repeat(64),"byte_size":"01"}]);
    assert!(matches!(inspect(&v), Err(BundleError::Shape)));
    let mut v = input();
    v["records"] = Value::Array(vec![v["records"][0].clone(); 10_001]);
    assert!(inspect(&v).is_err());
}

#[test]
fn enrollment_workspace_anchor_is_exact_and_is_not_authentication() {
    let v = json!({"schema_version":"1.0","purpose":"enrollment","root":{"table":"workspaces","id":WORKSPACE},
        "records":[{"table":"workspaces","id":WORKSPACE,"record_schema":"server-v1:S0","record":{"id":WORKSPACE}}],"referenced_blobs":[]});
    let candidate = inspect(&v).unwrap();
    assert_eq!(candidate.purpose(), Purpose::Enrollment);
    assert!(matches!(
        BundleCandidate::inspect(candidate.canonical_bytes(), &DomainId::new(ID).unwrap()),
        Err(BundleError::Workspace)
    ));
    // Intentionally incomplete record shapes can be inspected, but this type
    // has no API to register identities, validate closure or publish anything.
    assert_eq!(candidate.records().len(), 1);
}
