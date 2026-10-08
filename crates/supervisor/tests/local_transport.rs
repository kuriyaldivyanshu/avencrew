use avencrew_contracts::{
    canonical_bytes, decode_client_request, decode_internal_message, scalars::DomainId,
};
use avencrew_store_sqlite::LocalStore;
use avencrew_supervisor::{handshake_proof, LaunchContext, LaunchGrant, Supervisor};
use hmac::digest::Digest as _;
use serde_json::{json, Value};
use sha2_auth::Sha256;
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};
fn id(n: u32) -> String {
    format!("01900000-0000-7000-8000-{n:012x}")
}
fn domain(n: u32) -> DomainId {
    DomainId::new(id(n)).unwrap()
}
fn canonical(v: &Value) -> Result<Vec<u8>, ()> {
    canonical_bytes(&serde_json::to_vec(v).unwrap()).map_err(|_| ())
}
fn digest(b: &[u8]) -> [u8; 32] {
    Sha256::digest(b).into()
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let p = PathBuf::from(format!(
            "/tmp/avencrew-socket-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&p).unwrap();
        Self(p.canonicalize().unwrap())
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn run(f: impl std::future::Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}
async fn seed(root: &Root) {
    let mut s = LocalStore::open(&root.0).await.unwrap();
    s.bootstrap_standalone(&serde_json::to_vec(&fixture()).unwrap(), "keychain:test")
        .await
        .unwrap();
    s.close().await.unwrap();
}
fn context() -> LaunchContext {
    LaunchContext {
        schema_version: "avencrew.local-launch/1".into(),
        workspace_id: domain(1),
        actor_id: domain(2),
        main_pid: std::process::id(),
        client_build_digest: avencrew_contracts::scalars::Digest::new("a".repeat(64)).unwrap(),
    }
}
fn request(op: &str, body: Value, n: u32) -> Value {
    json!({"message_kind":"rpc_request","schema_version":"1.0","payload":{"frame_kind":"request","operation":op,"request_id":id(n),"body":body}})
}
fn handshake(grant: &LaunchGrant, connection_challenge: &str, n: u32) -> Value {
    let mut v = request(
        "handshake",
        json!({"supported_versions":["1.0"],"client_kind":"electron_main","build_digest":"a".repeat(64),"nonce":format!("{n:064x}"),"challenge_response":"","requested_workspace_id":id(1)}),
        n,
    );
    let uid = nix::unistd::geteuid().as_raw();
    let r = decode_client_request(&serde_json::to_vec(&v).unwrap()).unwrap();
    v["payload"]["body"]["challenge_response"] =
        json!(handshake_proof(grant, connection_challenge, uid, std::process::id(), &r).unwrap());
    v
}
async fn read_challenge(s: &mut UnixStream) -> String {
    let mut h = [0; 4];
    tokio::time::timeout(std::time::Duration::from_secs(3), s.read_exact(&mut h))
        .await
        .unwrap()
        .unwrap();
    let mut b = vec![0; u32::from_be_bytes(h) as usize];
    s.read_exact(&mut b).await.unwrap();
    serde_json::from_slice(&b).unwrap()
}
async fn send(s: &mut UnixStream, v: &Value) {
    let b = serde_json::to_vec(v).unwrap();
    s.write_all(&(b.len() as u32).to_be_bytes()).await.unwrap();
    s.write_all(&b).await.unwrap();
}
async fn receive(s: &mut UnixStream) -> Value {
    let mut h = [0; 4];
    tokio::time::timeout(std::time::Duration::from_secs(3), s.read_exact(&mut h))
        .await
        .unwrap()
        .unwrap();
    let mut b = vec![0; u32::from_be_bytes(h) as usize];
    s.read_exact(&mut b).await.unwrap();
    decode_internal_message(&b).unwrap();
    serde_json::from_slice(&b).unwrap()
}
async fn rejected(s: &mut UnixStream, v: &Value) {
    let _ = read_challenge(s).await;
    send(s, v).await;
    let mut h = [0; 4];
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(3), s.read_exact(&mut h))
            .await
            .unwrap()
            .is_err()
    );
}
fn fixture() -> Value {
    let now = "2026-10-07T00:00:00.000000Z";
    let audience = json!({"allowed_principal_ids":[id(3)]});
    let execution = json!({"default":"deny","allowed_tools":[]});
    let records = json!({
        "user_reference":{"id":id(4),"created_at":now,"row_version":"1","updated_at":now,"display_name":"Human","status":"active","erased_at":null},
        "principal":{"id":id(3),"workspace_id":id(1),"created_at":now,"row_version":"1","updated_at":now,"kind":"human","user_id":id(4),"display_name":"Human","status":"active"},
        "workspace":{"id":id(1),"created_at":now,"row_version":"1","updated_at":now,"name":"Local","origin_device_key":"abcd","status":"active","permission_generation":"0","deletion_generation":"0","settings":{},"last_observation_seq":"0","last_sync_seq":"0"},
        "device":{"id":id(5),"workspace_id":id(1),"created_at":now,"row_version":"1","updated_at":now,"principal_id":id(3),"public_key":"abcd","label":"Device","os":"macos","architecture":"arm64","status":"active","last_seen_at":null,"minimum_deletion_generation":"0"},
        "audience_policy":{"id":id(6),"workspace_id":id(1),"created_at":now,"name":"Owner only","version_no":1,"policy_kind":"audience","schema_version":1,"rules":audience,"digest":hex(&digest(&canonical(&audience).unwrap())),"published_by_id":id(3)},
        "execution_policy":{"id":id(7),"workspace_id":id(1),"created_at":now,"name":"Deny dispatch","version_no":1,"policy_kind":"execution","schema_version":1,"rules":execution,"digest":hex(&digest(&canonical(&execution).unwrap())),"published_by_id":id(3)}
    });
    let mut refs = serde_json::Map::new();
    for (i, name) in [
        "user_reference",
        "principal",
        "workspace",
        "device",
        "audience_policy",
        "execution_policy",
    ]
    .into_iter()
    .enumerate()
    {
        let bytes = canonical(&records[name]).unwrap();
        refs.insert(name.into(),json!({"blob_id":id(20+i as u32),"record_id":records[name]["id"],"record_schema":"server-v1:target","sha256":hex(&digest(&bytes)),"byte_size":bytes.len().to_string()}));
    }
    json!({"registration":{"schema_version":"avencrew.local-registration/1.0","id":id(10),"workspace_id":id(1),"actor_id":id(2),"device_id":id(5),"principal_id":id(3),"user_reference_id":id(4),"audience_policy_id":id(6),"execution_policy_id":id(7),"created_at":now,"origin_public_key":"abcd","permission_generation":"0","deletion_generation":"0","records":refs},"records":records})
}

#[test]
fn reconnects_do_not_exhaust_authentication_and_idle_frames_remain_bounded() {
    run(async {
        let root = Root::new();
        seed(&root).await;
        let supervisor = Supervisor::open(&root.0, context()).await.unwrap();
        let grant: LaunchGrant =
            serde_json::from_value(serde_json::to_value(supervisor.grant()).unwrap()).unwrap();
        let (stop, shutdown) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(supervisor.serve(shutdown));
        let mut previous_challenge = None;
        // Cross the former lifetime cap, even using the same client nonce.
        // Each connection instead authenticates against its own server challenge.
        for _ in 0..1026 {
            let mut socket = UnixStream::connect(&grant.socket_path).await.unwrap();
            let challenge = read_challenge(&mut socket).await;
            assert_ne!(previous_challenge.as_ref(), Some(&challenge));
            send(&mut socket, &handshake(&grant, &challenge, 40)).await;
            assert_eq!(receive(&mut socket).await["payload"]["ok"], true);
            previous_challenge = Some(challenge);
        }
        let mut socket = UnixStream::connect(&grant.socket_path).await.unwrap();
        let challenge = read_challenge(&mut socket).await;
        send(&mut socket, &handshake(&grant, &challenge, 40)).await;
        receive(&mut socket).await;
        tokio::time::sleep(std::time::Duration::from_secs(6)).await;
        send(
            &mut socket,
            &request("describe_capabilities", json!({}), 41),
        )
        .await;
        assert_eq!(receive(&mut socket).await["payload"]["ok"], true);
        // The first byte starts the five-second frame deadline, even though
        // idle waiting itself allows sixty seconds. An incomplete header closes.
        socket.write_all(&[0]).await.unwrap();
        let mut byte = [0];
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(7), socket.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        // A frame timeout affects only that client, not future authentication.
        let mut socket = UnixStream::connect(&grant.socket_path).await.unwrap();
        let challenge = read_challenge(&mut socket).await;
        send(&mut socket, &handshake(&grant, &challenge, 40)).await;
        assert_eq!(receive(&mut socket).await["payload"]["ok"], true);
        drop(socket);
        stop.send(()).unwrap();
        task.await.unwrap().unwrap();
    });
}

#[test]
fn authenticated_reconnect_replay_and_unauthorized_frames() {
    run(async {
        let root = Root::new();
        seed(&root).await;
        let supervisor = Supervisor::open(&root.0, context()).await.unwrap();
        let grant: LaunchGrant =
            serde_json::from_value(serde_json::to_value(supervisor.grant()).unwrap()).unwrap();
        assert_eq!(
            fs::metadata(&grant.socket_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(Supervisor::open(&root.0, context()).await.is_err());
        let (stop, shutdown) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(supervisor.serve(shutdown));
        let mut s = UnixStream::connect(&grant.socket_path).await.unwrap();
        let challenge = read_challenge(&mut s).await;
        let good = handshake(&grant, &challenge, 40);
        send(&mut s, &good).await;
        assert_eq!(receive(&mut s).await["payload"]["ok"], true);
        send(&mut s, &request("describe_capabilities", json!({}), 41)).await;
        let caps = receive(&mut s).await;
        assert_eq!(
            caps["payload"]["result"]["operation_names"],
            json!(["handshake", "describe_capabilities", "submit_command"])
        );
        assert_eq!(caps["payload"]["result"]["profiles"][1]["enabled"], false);
        drop(s);
        // Subscriber closure never shuts down the independently owned supervisor.
        // Replaying a prior connection's proof fails: the server issues a fresh challenge.
        let mut s = UnixStream::connect(&grant.socket_path).await.unwrap();
        rejected(&mut s, &good).await;
        for mode in 0..5 {
            let mut s = UnixStream::connect(&grant.socket_path).await.unwrap();
            let challenge = read_challenge(&mut s).await;
            let mut v = handshake(&grant, &challenge, 50 + mode);
            match mode {
                0 => v["payload"]["body"]["challenge_response"] = json!("0".repeat(64)),
                1 => v["payload"]["body"]["requested_workspace_id"] = json!(id(99)),
                2 => v["payload"]["body"]["build_digest"] = json!("b".repeat(64)),
                3 => v["payload"]["body"]["supported_versions"] = json!(["2.0"]),
                _ => v["payload"]["body"]["actor_id"] = json!(id(2)),
            };
            // Challenge already consumed above; send the tampered frame directly.
            send(&mut s, &v).await;
            let mut h = [0; 4];
            assert!(
                tokio::time::timeout(std::time::Duration::from_secs(3), s.read_exact(&mut h))
                    .await
                    .unwrap()
                    .is_err()
            );
        }
        let mut s = UnixStream::connect(&grant.socket_path).await.unwrap();
        rejected(&mut s, &request("describe_capabilities", json!({}), 70)).await;
        let mut s = UnixStream::connect(&grant.socket_path).await.unwrap();
        let challenge = read_challenge(&mut s).await;
        send(&mut s, &handshake(&grant, &challenge, 71)).await;
        receive(&mut s).await;
        send(&mut s,&request("submit_command",json!({"kind":"stop","command_id":id(500),"run_id":id(501),"idempotency_key":"stop","payload":{"reason":"stop"}}),72)).await;
        let reply = receive(&mut s).await;
        assert_eq!(reply["payload"]["ok"], false);
        assert_eq!(reply["payload"]["error"]["code"], "NOT_FOUND_OR_DENIED");
        drop(s);
        // Wrong PID is checked by OS credentials before a frame/proof is read.
        let child=std::process::Command::new("/usr/bin/python3").args(["-c","import socket,sys; s=socket.socket(socket.AF_UNIX); s.connect(sys.argv[1]); assert s.recv(1)==b''",grant.socket_path.to_str().unwrap()]).spawn().unwrap();
        let waiter = tokio::task::spawn_blocking(move || child.wait_with_output().unwrap());
        assert!(waiter.await.unwrap().status.success());
        stop.send(()).unwrap();
        task.await.unwrap().unwrap();
        assert!(!grant.socket_path.exists());
        let s = LocalStore::open(&root.0).await.unwrap();
        s.close().await.unwrap();
    });
}

#[test]
fn real_process_private_pipe_launch_crash_and_stale_endpoint_restart() {
    run(async {
        let root = Root::new();
        seed(&root).await;
        let mut previous_grant = None;
        for n in 0..2 {
            let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_avencrew-supervisor"))
                .args(["serve", "--data-root", root.0.to_str().unwrap()])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let b = serde_json::to_vec(&context()).unwrap();
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(&(b.len() as u32).to_be_bytes())
                .unwrap();
            child.stdin.as_mut().unwrap().write_all(&b).unwrap();
            let mut h = [0; 4];
            child.stdout.as_mut().unwrap().read_exact(&mut h).unwrap();
            let mut b = vec![0; u32::from_be_bytes(h) as usize];
            child.stdout.as_mut().unwrap().read_exact(&mut b).unwrap();
            let grant: LaunchGrant = serde_json::from_slice(&b).unwrap();
            assert_eq!(grant.generation, (n + 1).to_string());
            if let Some(old) = previous_grant.as_ref() {
                let mut s = UnixStream::connect(&grant.socket_path).await.unwrap();
                let challenge = read_challenge(&mut s).await;
                // Old grant proof against the new connection challenge must fail.
                send(&mut s, &handshake(old, &challenge, 90 + n)).await;
                let mut h = [0; 4];
                assert!(tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    s.read_exact(&mut h)
                )
                .await
                .unwrap()
                .is_err());
            }
            // Launch-channel EOF is independent of the service/window lifetime.
            drop(child.stdin.take());
            let mut s = UnixStream::connect(&grant.socket_path).await.unwrap();
            let challenge = read_challenge(&mut s).await;
            send(&mut s, &handshake(&grant, &challenge, 80 + n)).await;
            receive(&mut s).await;
            drop(s);
            assert!(LocalStore::open(&root.0).await.is_err());
            child.kill().unwrap();
            let out = child.wait_with_output().unwrap();
            assert!(!String::from_utf8_lossy(&out.stderr).contains(&grant.session_key));
            assert!(out.stderr.is_empty());
            assert!(grant.socket_path.exists());
            previous_grant = Some(grant);
        }
    });
}

fn task_bundle() -> Value {
    let task = true;
    let raw = false;
    let base = 100;
    let time = "2026-10-08T00:00:00.000000Z";
    let body = json!({"text":"Explicit outcome, acceptance criteria and evidence"});
    let content = if raw {
        digest(b"payload")
    } else {
        digest(&canonical(&body).unwrap())
    };
    let spec = json!({"schema_version":1,"success_criteria":"Human reviews the exact version against the requested outcome"});
    let resource = json!({"id":id(base+1),"workspace_id":id(1),"created_at":time,"row_version":"1","updated_at":time,"project_id":null,"owner_id":id(3),"kind":if task {"task"}else{"artifact"},"visibility":"private","status":"active","acl_generation":"1","current_version_id":id(base+2)});
    let version = json!({"id":id(base+2),"workspace_id":id(1),"created_at":time,"resource_id":id(base+1),"version_no":"1","blob_id":if raw {json!(id(base+6))}else{Value::Null},"inline_payload":if raw {Value::Null}else{body},"content_digest":hex(&content),"observed_at":time,"effective_at":null,"fresh_until":null,"source_etag":null,"created_by_id":id(3),"availability":"present","erased_at":null});
    let check = json!({"id":id(base+4),"workspace_id":id(1),"created_at":time,"resource_id":id(base+1),"version_no":1,"kind":"human","spec":spec,"digest":hex(&digest(&canonical(&spec).unwrap())),"required":true});
    let mut records = vec![
        wrapper("resources", resource),
        wrapper("resource_versions", version),
        wrapper("checks", check),
    ];
    records.push(wrapper("tasks",json!({"id":id(base),"workspace_id":id(1),"created_at":time,"row_version":"1","updated_at":time,"resource_id":id(base+1),"owner_id":id(3),"current_revision_id":id(base+3),"status":"open","priority":0})));
    records.push(wrapper("task_revisions",json!({"id":id(base+3),"workspace_id":id(1),"created_at":time,"task_id":id(base),"version_no":1,"objective_version_id":id(base+2),"acting_principal_id":id(3),"audience_policy_id":id(6),"execution_policy_id":id(7),"completion_mode":"bounded","accepted_by_id":id(3),"accepted_at":time})));
    records.push(wrapper("task_checks",json!({"id":id(base+5),"workspace_id":id(1),"created_at":time,"task_revision_id":id(base+3),"check_id":id(base+4),"required":true})));

    json!({"schema_version":"1.0","purpose":"task","root":{"table":"tasks","id":id(100)},"records":records,"referenced_blobs":[]})
}
fn wrapper(table: &str, record: Value) -> Value {
    json!({"table":table,"id":record["id"],"record_schema":"server-v1:S0","record":record})
}

#[test]
fn committed_command_survives_lost_reply_and_reconnect() {
    run(async {
        let root = Root::new();
        seed_run(&root).await;
        let supervisor = Supervisor::open(&root.0, context()).await.unwrap();
        let grant: LaunchGrant =
            serde_json::from_value(serde_json::to_value(supervisor.grant()).unwrap()).unwrap();
        let (stop, shutdown) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(supervisor.serve(shutdown));
        let mut s = UnixStream::connect(&grant.socket_path).await.unwrap();
        let challenge = read_challenge(&mut s).await;
        send(&mut s, &handshake(&grant, &challenge, 110)).await;
        receive(&mut s).await;
        let command = request(
            "submit_command",
            json!({"kind":"steer","command_id":id(400),"run_id":id(300),"idempotency_key":"socket-control","payload":{"text":"Keep the requested outcome","expected_revision":"1"}}),
            111,
        );
        send(&mut s, &command).await;
        let original = receive(&mut s).await;
        assert_eq!(original["payload"]["ok"], true);
        assert_eq!(original["payload"]["result"]["accepted_seq"], "1");
        // Retry and disconnect without consuming the committed reply.
        send(&mut s, &command).await;
        drop(s);
        let mut s = UnixStream::connect(&grant.socket_path).await.unwrap();
        let challenge = read_challenge(&mut s).await;
        send(&mut s, &handshake(&grant, &challenge, 112)).await;
        receive(&mut s).await;
        send(&mut s, &command).await;
        assert_eq!(receive(&mut s).await, original);
        let mut drift = command.clone();
        drift["payload"]["body"]["payload"]["text"] = json!("Changed text");
        send(&mut s, &drift).await;
        assert_eq!(
            receive(&mut s).await["payload"]["error"]["code"],
            "IDEMPOTENCY_MISMATCH"
        );
        drop(s);
        stop.send(()).unwrap();
        task.await.unwrap().unwrap();
    });
}

#[test]
fn invalid_frames_and_runtime_metadata_fail_closed() {
    run(async {
        let root = Root::new();
        seed(&root).await;
        let supervisor = Supervisor::open(&root.0, context()).await.unwrap();
        let grant: LaunchGrant =
            serde_json::from_value(serde_json::to_value(supervisor.grant()).unwrap()).unwrap();
        let (stop, shutdown) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(supervisor.serve(shutdown));
        for length in [0, 1_048_577] {
            let mut s = UnixStream::connect(&grant.socket_path).await.unwrap();
            let _ = read_challenge(&mut s).await;
            s.write_all(&u32::to_be_bytes(length)).await.unwrap();
            let mut b = [0];
            assert_eq!(s.read(&mut b).await.unwrap(), 0);
        }
        let mut s = UnixStream::connect(&grant.socket_path).await.unwrap();
        let _ = read_challenge(&mut s).await;
        s.write_all(&10u32.to_be_bytes()).await.unwrap();
        s.write_all(b"{").await.unwrap();
        s.shutdown().await.unwrap();
        let mut b = [0];
        assert_eq!(s.read(&mut b).await.unwrap(), 0);
        for bytes in [
            b"{\"message_kind\":\"rpc_request\",\"message_kind\":\"rpc_request\"}".as_slice(),
            b"{\"message_kind\":\"model_request\"}".as_slice(),
        ] {
            let mut s = UnixStream::connect(&grant.socket_path).await.unwrap();
            let _ = read_challenge(&mut s).await;
            s.write_all(&(bytes.len() as u32).to_be_bytes())
                .await
                .unwrap();
            s.write_all(bytes).await.unwrap();
            assert_eq!(s.read(&mut b).await.unwrap(), 0);
        }
        stop.send(()).unwrap();
        task.await.unwrap().unwrap();
        let generation = root.0.join("control/supervisor.generation");
        for bad in ["01", "0", "9223372036854775807", "garbage"] {
            fs::write(&generation, bad).unwrap();
            let error = Supervisor::open(&root.0, context()).await.err().unwrap();
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
            let store = LocalStore::open(&root.0).await.unwrap();
            store.close().await.unwrap();
        }
        fs::write(&generation, "1").unwrap();
        fs::set_permissions(&generation, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(Supervisor::open(&root.0, context()).await.is_err());
    });
}

#[test]
fn node_launcher_inherited_socket_pair_and_cross_language_proof() {
    run(async {
        let root = Root::new();
        seed(&root).await;
        let output = std::process::Command::new("node")
            .args([
                "--input-type=module",
                "-e",
                include_str!("node_launcher.mjs"),
                env!("CARGO_BIN_EXE_avencrew-supervisor"),
                root.0.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"node launch and proof verified\n");
    });
}

async fn seed_run(root: &Root) {
    use avencrew_contracts::scalars::Instant;
    use avencrew_store_sqlite::{BundlePublication, LocalRunRegistration, RecordAllocation};
    seed(root).await;
    let mut store = LocalStore::open(&root.0).await.unwrap();
    let bundle = task_bundle();
    let bytes = canonical(&bundle).unwrap();
    let allocations: Vec<_> = bundle["records"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(n, r)| RecordAllocation {
            table: r["table"].as_str().unwrap().into(),
            id: DomainId::new(r["id"].as_str().unwrap()).unwrap(),
            blob_id: domain(1000 + n as u32),
        })
        .collect();
    store
        .publish_bundle(BundlePublication {
            workspace: &domain(1),
            actor: &domain(2),
            bundle_blob_id: &domain(900),
            bytes: &bytes,
            sha256: digest(&bytes),
            record_blobs: &allocations,
            payloads: &[],
        })
        .await
        .unwrap();
    store
        .register_local_run(LocalRunRegistration {
            workspace: &domain(1),
            actor: &domain(2),
            task: &domain(100),
            task_revision: &domain(103),
            run: &domain(300),
            event: &domain(301),
            queue: &domain(302),
            payload_blob: &domain(303),
            at: &Instant::new("2026-10-08T01:00:00.000000Z").unwrap(),
        })
        .await
        .unwrap();
    store.close().await.unwrap();
}

#[test]
fn startup_restores_committed_control_and_denies_corrupt_history_before_grant() {
    run(async {
        use avencrew_contracts::scalars::Instant;
        use avencrew_store_sqlite::{CommandAcceptance, RecoveryGate};
        let root = Root::new();
        seed_run(&root).await;
        let mut store = LocalStore::open(&root.0).await.unwrap();
        let bytes=serde_json::to_vec(&json!({"kind":"steer","command_id":id(400),"run_id":id(300),"idempotency_key":"restart-control","payload":{"text":"Retain this exact steering","expected_revision":"1"}})).unwrap();
        let receipt = store
            .accept_command(CommandAcceptance {
                workspace: &domain(1),
                actor: &domain(2),
                bytes: &bytes,
                event: &domain(401),
                queue: &domain(402),
                command_blob: &domain(403),
                event_blob: &domain(404),
                at: &Instant::new("2026-10-08T01:00:00.000000Z").unwrap(),
            })
            .await
            .unwrap();
        store.close().await.unwrap();
        let supervisor = Supervisor::open(&root.0, context()).await.unwrap();
        let restored = &supervisor.startup_recovery().runs[0];
        assert_eq!(restored.gate, RecoveryGate::AwaitingControlConsumer);
        assert_eq!(restored.applied.value(), 0);
        assert_eq!(restored.pending_controls[0].original_receipt, receipt);
        assert_eq!(
            restored.pending_controls[0].canonical_bytes,
            canonical_bytes(&bytes).unwrap()
        );
        assert_eq!(
            canonical_bytes(&serde_json::to_vec(&restored.pending_controls[0].command).unwrap())
                .unwrap(),
            restored.pending_controls[0].canonical_bytes
        );
        let (stop, shutdown) = tokio::sync::oneshot::channel();
        stop.send(()).unwrap();
        supervisor.serve(shutdown).await.unwrap();
        fs::write(
            root.0.join(format!("blobs/{}/{}.json", id(1), id(403))),
            b"corrupt",
        )
        .unwrap();
        assert!(Supervisor::open(&root.0, context()).await.is_err());
        assert!(!root.0.join("control/sock").exists());
        let store = LocalStore::open(&root.0).await.unwrap();
        store.close().await.unwrap();
    });
}
