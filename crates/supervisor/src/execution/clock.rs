//! OS clock observations never come from a model or saved lease.
use avencrew_contracts::scalars::{Counter, DomainId, Instant};
use sha2_auth::{Digest as _, Sha256};
use std::io;

pub(super) struct Sample {
    pub now: Instant,
    pub boot: DomainId,
    pub monotonic: Counter,
}

pub(super) fn sample() -> io::Result<Sample> {
    #[cfg(target_os = "macos")]
    let boot = {
        let output = std::process::Command::new("/usr/sbin/sysctl")
            .args(["-n", "kern.bootsessionuuid"])
            .output()?;
        if !output.status.success() || output.stdout.len() > 256 {
            return Err(io::Error::other("boot observation unavailable"));
        }
        output.stdout
    };
    #[cfg(target_os = "linux")]
    let boot = std::fs::read("/proc/sys/kernel/random/boot_id")?;
    // DomainId is the wire's UUIDv7-shaped identifier. This deterministic alias
    // identifies the observed OS boot; it does not allocate an execution lease.
    let hash = Sha256::digest(&boot);
    let mut bytes: [u8; 16] = hash[..16]
        .try_into()
        .map_err(|_| io::Error::other("boot observation"))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let boot = DomainId::new(uuid::Uuid::from_bytes(bytes).to_string())
        .map_err(|_| io::Error::other("boot observation"))?;
    let t =
        nix::time::clock_gettime(nix::time::ClockId::CLOCK_MONOTONIC).map_err(io::Error::from)?;
    let nanos = t
        .tv_sec()
        .checked_mul(1_000_000_000)
        .and_then(|n| n.checked_add(t.tv_nsec()))
        .ok_or_else(|| io::Error::other("monotonic range"))?;
    let monotonic =
        Counter::new(nanos.to_string()).map_err(|_| io::Error::other("monotonic range"))?;
    let micros = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| io::Error::other("wall clock"))?
        .as_micros();
    let now = chrono::DateTime::from_timestamp_micros(
        i64::try_from(micros).map_err(|_| io::Error::other("wall clock range"))?,
    )
    .ok_or_else(|| io::Error::other("wall clock range"))?;
    Ok(Sample {
        now: Instant::new(now.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string())
            .map_err(|_| io::Error::other("wall clock format"))?,
        boot,
        monotonic,
    })
}
