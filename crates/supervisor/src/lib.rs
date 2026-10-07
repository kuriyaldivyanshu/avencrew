//! Restricted supervisor control transport. No renderer, SQL or execution bridge.
use avencrew_contracts::{
    canonical_bytes, decode_client_request, decode_internal_message, encode_message,
    scalars::{Digest, DomainId, Instant},
    wire::{RpcRequest, RpcRequestHandshakeBodyClientKind},
};
use avencrew_store_sqlite::{CommandAcceptance, ControlError, LocalStore};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2_auth::{Digest as _, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::{oneshot, Mutex},
};

const MAX: usize = 1_048_576;
const DEADLINE: Duration = Duration::from_secs(5);
const MANIFEST: &str = r#"{"operation_names":["handshake","describe_capabilities","submit_command"],"profiles":[{"id":"local-controls-steer-pause-stop","enabled":true,"reason":null},{"id":"harness-execution","enabled":false,"reason":"Harness and executable admission are not implemented"}],"schema_versions":["sqlite:L0-L1"]}"#;
fn denied() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "local supervisor authentication or scope denied",
    )
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid local supervisor protocol or runtime state",
    )
}
fn store_error(_: impl std::fmt::Display) -> io::Error {
    io::Error::other("local journal unavailable; no acceptance claimed")
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn hash(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
fn random() -> io::Result<[u8; 32]> {
    let mut b = [0; 32];
    getrandom::fill(&mut b).map_err(|_| io::Error::other("OS entropy unavailable"))?;
    Ok(b)
}
fn id() -> DomainId {
    DomainId::new(uuid::Uuid::now_v7().to_string()).expect("UUIDv7 generator")
}
fn instant() -> io::Result<Instant> {
    let micros = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| invalid())?
        .as_micros();
    let t = chrono::DateTime::from_timestamp_micros(i64::try_from(micros).map_err(|_| invalid())?)
        .ok_or_else(invalid)?;
    Instant::new(t.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()).map_err(|_| invalid())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchContext {
    pub schema_version: String,
    pub workspace_id: DomainId,
    pub actor_id: DomainId,
    pub main_pid: u32,
    pub client_build_digest: Digest,
}
/// Private pipe payload. Deliberately has no Debug or log representation.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchGrant {
    pub schema_version: String,
    pub socket_path: PathBuf,
    pub generation: String,
    pub scope_id: DomainId,
    pub challenge: String,
    pub session_key: String,
}
struct Session {
    context: LaunchContext,
    grant: LaunchGrant,
    key: [u8; 32],
    uid: u32,
    nonces: Mutex<BTreeSet<String>>,
    build: String,
    manifest: String,
}
struct Endpoint {
    path: PathBuf,
    ino: u64,
    dev: u64,
}
impl Endpoint {
    fn remove(&self) -> io::Result<()> {
        let m = fs::symlink_metadata(&self.path)?;
        if !m.file_type().is_socket() || m.ino() != self.ino || m.dev() != self.dev {
            return Err(invalid());
        }
        fs::remove_file(&self.path)?;
        File::open(self.path.parent().ok_or_else(invalid)?)?.sync_all()
    }
}
fn private(path: &Path, uid: u32, dir: bool) -> io::Result<()> {
    let m = fs::symlink_metadata(path)?;
    if m.uid() != uid
        || m.mode() & 0o777 != if dir { 0o700 } else { 0o600 }
        || if dir {
            !m.is_dir()
        } else {
            !m.is_file() || m.nlink() != 1
        }
    {
        return Err(denied());
    }
    Ok(())
}
fn generation(root: &Path, uid: u32) -> io::Result<String> {
    let p = root.join("supervisor.generation");
    let previous = match fs::symlink_metadata(&p) {
        Ok(_) => {
            private(&p, uid, false)?;
            if fs::metadata(&p)?.len() > 19 {
                return Err(invalid());
            }
            let bytes = fs::read(&p)?;
            if bytes.len() > 19 {
                return Err(invalid());
            }
            let text = std::str::from_utf8(&bytes).map_err(|_| invalid())?;
            let n = text.parse::<i64>().map_err(|_| invalid())?;
            if n < 1 || n.to_string() != text {
                return Err(invalid());
            }
            n
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => 0,
        Err(e) => return Err(e),
    };
    let next = previous.checked_add(1).ok_or_else(invalid)?.to_string();
    let pending = root.join("supervisor.generation.pending");
    if pending.exists() {
        private(&pending, uid, false)?;
        fs::remove_file(&pending)?;
    }
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&pending)?;
    f.write_all(next.as_bytes())?;
    f.sync_all()?;
    fs::rename(pending, p)?;
    File::open(root)?.sync_all()?;
    Ok(next)
}
fn binary_digest() -> io::Result<String> {
    let mut f = File::open(std::env::current_exe()?)?;
    let mut h = Sha256::new();
    let mut buf = [0; 16384];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}
fn transcript(
    grant: &LaunchGrant,
    uid: u32,
    pid: u32,
    request: &RpcRequest,
) -> io::Result<Vec<u8>> {
    let mut value = serde_json::to_value(request).map_err(|_| invalid())?;
    value["body"]["challenge_response"] = json!("");
    canonical_bytes(&serde_json::to_vec(&json!({"domain":"avencrew.local-handshake/1","challenge":grant.challenge,"generation":grant.generation,"scope_id":grant.scope_id,"peer_uid":uid,"peer_pid":pid,"request":value})).map_err(|_|invalid())?).map_err(|_|invalid())
}
fn unhex(s: &str) -> io::Result<Vec<u8>> {
    if s.len() != 64
        || !s
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(denied());
    }
    (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| denied()))
        .collect()
}
impl Session {
    async fn authenticate(&self, request: &RpcRequest) -> io::Result<()> {
        let RpcRequest::Handshake { body, .. } = request else {
            return Err(denied());
        };
        if body.client_kind != RpcRequestHandshakeBodyClientKind::ElectronMain
            || body.build_digest != self.context.client_build_digest
            || body.requested_workspace_id.0.as_ref() != Some(&self.context.workspace_id)
            || !body
                .supported_versions
                .as_slice()
                .iter()
                .any(|v| v.as_str() == "1.0")
        {
            return Err(denied());
        }
        unhex(body.nonce.as_str())?;
        let proof = unhex(body.challenge_response.as_str())?;
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.key).map_err(|_| invalid())?;
        mac.update(&transcript(
            &self.grant,
            self.uid,
            self.context.main_pid,
            request,
        )?);
        mac.verify_slice(&proof).map_err(|_| denied())?;
        let mut nonces = self.nonces.lock().await;
        if nonces.len() >= 1024 || !nonces.insert(body.nonce.as_str().into()) {
            return Err(denied());
        }
        Ok(())
    }
}
/// Bound both memory and slow/partial frames. State changes require full decoding.
async fn read_frame(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
    tokio::time::timeout(DEADLINE, async {
        let mut h = [0; 4];
        stream.read_exact(&mut h).await?;
        let n = u32::from_be_bytes(h) as usize;
        if n == 0 || n > MAX {
            return Err(invalid());
        }
        let mut bytes = vec![0; n];
        stream.read_exact(&mut bytes).await?;
        Ok(bytes)
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "local frame timeout"))?
}
async fn reply(
    stream: &mut UnixStream,
    request: &RpcRequest,
    result: Result<Value, &str>,
) -> io::Result<()> {
    let original = serde_json::to_value(request).map_err(|_| invalid())?;
    let mut payload = json!({"frame_kind":"response","operation":original["operation"],"request_id":original["request_id"],"ok":result.is_ok()});
    match result {
        Ok(value) => payload["result"] = value,
        Err(code) => {
            payload["error"] = json!({"code":code,"message":"Operation unavailable or denied in this profile","evidence_refs":[]})
        }
    };
    let value = json!({"message_kind":"rpc_response","schema_version":"1.0","payload":payload});
    let message = decode_internal_message(&serde_json::to_vec(&value).map_err(|_| invalid())?)
        .map_err(|_| invalid())?;
    let bytes = encode_message(&message).map_err(|_| invalid())?;
    tokio::time::timeout(DEADLINE, async {
        stream
            .write_all(&(bytes.len() as u32).to_be_bytes())
            .await?;
        stream.write_all(&bytes).await
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "local reply timeout"))?
}
async fn client(
    mut stream: UnixStream,
    session: Arc<Session>,
    store: Arc<Mutex<LocalStore>>,
) -> io::Result<()> {
    let request = decode_client_request(&read_frame(&mut stream).await?).map_err(|_| invalid())?;
    session.authenticate(&request).await?;
    store
        .lock()
        .await
        .resolve_standalone(&session.context.workspace_id, &session.context.actor_id)
        .await
        .map_err(store_error)?;
    reply(&mut stream,&request,Ok(json!({"selected_version":"1.0","supervisor_generation":session.grant.generation,"schema_min":"sqlite:L0-L1","schema_max":"sqlite:L0-L1","capability_manifest_digest":session.manifest,"max_frame_bytes":MAX,"authenticated_scope_id":session.grant.scope_id}))).await?;
    for _ in 0..256 {
        let request = match read_frame(&mut stream).await {
            Ok(bytes) => decode_client_request(&bytes).map_err(|_| invalid())?,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        };
        let result = match &request {
            RpcRequest::DescribeCapabilities { .. } => {
                store
                    .lock()
                    .await
                    .resolve_standalone(&session.context.workspace_id, &session.context.actor_id)
                    .await
                    .map_err(store_error)?;
                let mut manifest: Value = serde_json::from_str(MANIFEST).map_err(|_| invalid())?;
                manifest["protocol_version"] = json!("1.0");
                manifest["build_digest"] = json!(session.build);
                manifest["capability_manifest_digest"] = json!(session.manifest);
                Ok(manifest)
            }
            RpcRequest::SubmitCommand { body, .. } => {
                let bytes = serde_json::to_vec(body).map_err(|_| invalid())?;
                let event = id();
                let queue = id();
                let command_blob = id();
                let event_blob = id();
                let at = instant()?;
                let receipt = store
                    .lock()
                    .await
                    .accept_command(CommandAcceptance {
                        workspace: &session.context.workspace_id,
                        actor: &session.context.actor_id,
                        bytes: &bytes,
                        event: &event,
                        queue: &queue,
                        command_blob: &command_blob,
                        event_blob: &event_blob,
                        at: &at,
                    })
                    .await;
                match receipt {
                    Ok(r) => Ok(serde_json::to_value(r).map_err(|_| invalid())?),
                    Err(e) => Err(match e {
                        ControlError::InvalidCommand => "INVALID_ARGUMENT",
                        ControlError::Unsupported => "UNSUPPORTED",
                        ControlError::IdempotencyMismatch => "IDEMPOTENCY_MISMATCH",
                        ControlError::Conflict => "CONFLICT",
                        ControlError::StaleAuthority => "STALE_AUTHORITY",
                        ControlError::Cancelled => "CANCELLED",
                        ControlError::NotFoundOrDenied => "NOT_FOUND_OR_DENIED",
                        ControlError::Store(_) => "UNAVAILABLE",
                    }),
                }
            }
            _ => Err("UNSUPPORTED"),
        };
        reply(&mut stream, &request, result).await?;
    }
    Ok(())
}
/// Owns the journal and listener until explicit supervisor shutdown, not client EOF.
pub struct Supervisor {
    store: LocalStore,
    listener: UnixListener,
    endpoint: Endpoint,
    session: Arc<Session>,
}
impl Supervisor {
    pub async fn open(root: &Path, context: LaunchContext) -> io::Result<Self> {
        if context.schema_version != "avencrew.local-launch/1"
            || context.main_pid == 0
            || context.main_pid > i32::MAX as u32
        {
            return Err(invalid());
        }
        let uid = nix::unistd::geteuid().as_raw();
        let store = LocalStore::open(root).await.map_err(store_error)?;
        let mut store = store;
        let result: io::Result<_> = async {
            private(
                store.status().database_path.parent().ok_or_else(invalid)?,
                uid,
                true,
            )?;
            store
                .resolve_standalone(&context.workspace_id, &context.actor_id)
                .await
                .map_err(store_error)?;
            let root = store
                .status()
                .database_path
                .parent()
                .ok_or_else(invalid)?
                .to_path_buf();
            let runtime = root.join("control");
            match fs::create_dir(&runtime) {
                Ok(()) => fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            };
            private(&runtime, uid, true)?;
            let path = runtime.join("sock");
            if path.as_os_str().len() > 100 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Unix socket path exceeds the portable local limit",
                ));
            }
            match fs::symlink_metadata(&path) {
                Ok(m) => {
                    if !m.file_type().is_socket()
                        || m.uid() != uid
                        || m.mode() & 0o777 != 0o600
                        || m.nlink() != 1
                    {
                        return Err(denied());
                    }
                    match std::os::unix::net::UnixStream::connect(&path) {
                        Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                            fs::remove_file(&path)?
                        }
                        _ => return Err(denied()),
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            };
            let generation = generation(&runtime, uid)?;
            let key = random()?;
            let grant = LaunchGrant {
                schema_version: "avencrew.local-session/1".into(),
                socket_path: path.clone(),
                generation,
                scope_id: id(),
                challenge: hex(&random()?),
                session_key: hex(&key),
            };
            let listener = UnixListener::bind(&path)?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
            File::open(&runtime)?.sync_all()?;
            let m = fs::symlink_metadata(&path)?;
            let endpoint = Endpoint {
                path,
                ino: m.ino(),
                dev: m.dev(),
            };
            let session = Arc::new(Session {
                context,
                grant,
                key,
                uid,
                nonces: Mutex::new(BTreeSet::new()),
                build: binary_digest()?,
                manifest: hash(&canonical_bytes(MANIFEST.as_bytes()).map_err(|_| invalid())?),
            });
            Ok((listener, endpoint, session))
        }
        .await;
        match result {
            Ok((listener, endpoint, session)) => Ok(Self {
                store,
                listener,
                endpoint,
                session,
            }),
            Err(error) => {
                // Close SQLite before releasing ownership, even on rejected startup.
                store.close().await.map_err(store_error)?;
                Err(error)
            }
        }
    }
    pub fn grant(&self) -> &LaunchGrant {
        &self.session.grant
    }
    pub async fn serve(self, mut shutdown: oneshot::Receiver<()>) -> io::Result<()> {
        let Self {
            store,
            listener,
            endpoint,
            session,
        } = self;
        let store = Arc::new(Mutex::new(store));
        let mut tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();
        let mut accept_error = None;
        loop {
            match shutdown.try_recv() {
                Ok(()) | Err(oneshot::error::TryRecvError::Closed) => break,
                Err(oneshot::error::TryRecvError::Empty) => {}
            }
            tasks.retain(|t| !t.is_finished());
            match tokio::time::timeout(Duration::from_millis(100), listener.accept()).await {
                Ok(Ok((stream, _))) => {
                    let Ok(credentials) = stream.peer_cred() else {
                        drop(stream);
                        continue;
                    };
                    if credentials.uid() != session.uid
                        || credentials.pid() != Some(session.context.main_pid as i32)
                        || tasks.len() >= 16
                    {
                        drop(stream);
                        continue;
                    }
                    let s = Arc::clone(&session);
                    let store = Arc::clone(&store);
                    tasks.push(tokio::spawn(async move {
                        let _ = client(stream, s, store).await;
                    }));
                }
                Ok(Err(e)) => {
                    accept_error = Some(e);
                    break;
                }
                Err(_) => {}
            }
        }
        drop(listener);
        for t in tasks {
            t.abort();
            let _ = t.await;
        }
        let cleanup = endpoint.remove();
        let store = Arc::try_unwrap(store)
            .map_err(|_| io::Error::other("supervisor ownership still borrowed"))?
            .into_inner();
        store.close().await.map_err(store_error)?;
        cleanup?;
        match accept_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}
/// Used only by the trusted launcher. Both descriptors must be inherited private OS pipes or socket pairs.
pub fn read_launch_context() -> io::Result<LaunchContext> {
    use std::os::fd::AsFd;
    let stdin = io::stdin();
    let stdout = io::stdout();
    for fd in [stdin.as_fd(), stdout.as_fd()] {
        let m = File::from(fd.try_clone_to_owned()?).metadata()?;
        if !(m.file_type().is_fifo() || m.file_type().is_socket()) {
            return Err(denied());
        }
    }
    let mut input = io::stdin().lock();
    let mut h = [0; 4];
    input.read_exact(&mut h)?;
    let n = u32::from_be_bytes(h) as usize;
    if n == 0 || n > 4096 {
        return Err(invalid());
    }
    let mut bytes = vec![0; n];
    input.read_exact(&mut bytes)?;
    let canonical = canonical_bytes(&bytes).map_err(|_| invalid())?;
    let context: LaunchContext = serde_json::from_slice(&canonical).map_err(|_| invalid())?;
    validate_launch_io(context.main_pid)?;
    Ok(context)
}
fn validate_launch_io(pid: u32) -> io::Result<()> {
    use std::os::fd::{AsFd, BorrowedFd};
    fn check(fd: BorrowedFd<'_>, uid: u32) -> io::Result<()> {
        let m = File::from(fd.try_clone_to_owned()?).metadata()?;
        if m.uid() != uid || !(m.file_type().is_fifo() || m.file_type().is_socket()) {
            return Err(denied());
        }
        if m.file_type().is_socket() {
            // macOS anonymous socket pairs expose getpeereid, but not
            // LOCAL_PEEREPID. The launcher is instead bound to our OS parent.
            #[cfg(target_os = "macos")]
            let peer_uid = nix::unistd::getpeereid(fd)
                .map_err(io::Error::from)?
                .0
                .as_raw();
            #[cfg(not(target_os = "macos"))]
            let peer_uid = {
                let stream = std::os::unix::net::UnixStream::from(fd.try_clone_to_owned()?);
                stream.set_nonblocking(true)?;
                UnixStream::from_std(stream)?.peer_cred()?.uid()
            };
            if peer_uid != uid {
                return Err(denied());
            }
        }
        Ok(())
    }
    if nix::unistd::getppid().as_raw() != pid as i32 {
        return Err(denied());
    }
    let uid = nix::unistd::geteuid().as_raw();
    check(io::stdin().as_fd(), uid)?;
    check(io::stdout().as_fd(), uid)
}
/// Main-session client helper; the private pipe grant is required to make proof.
pub fn handshake_proof(
    grant: &LaunchGrant,
    uid: u32,
    pid: u32,
    request: &RpcRequest,
) -> io::Result<String> {
    let key = unhex(&grant.session_key)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(&key).map_err(|_| invalid())?;
    mac.update(&transcript(grant, uid, pid, request)?);
    Ok(hex(&mac.finalize().into_bytes()))
}
pub fn write_launch_grant(grant: &LaunchGrant) -> io::Result<()> {
    let bytes = serde_json::to_vec(grant).map_err(|_| invalid())?;
    let mut output = io::stdout().lock();
    output.write_all(&(bytes.len() as u32).to_be_bytes())?;
    output.write_all(&bytes)?;
    output.flush()
}
