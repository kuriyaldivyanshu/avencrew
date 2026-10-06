//! Native SQLite build prerequisite guard.
//!
//! Fails the build unless the managed native environment is complete and points
//! at *this* project's verified engine. The point is that a bare `cargo build`
//! can never quietly produce a binary linked against a host, Homebrew or
//! otherwise unexpected SQLite.
//!
//! Standard library only: no new build dependencies.
//!
//! This guard is not permission for `unsafe` Rust and adds no runtime service.
//! Note that Cargo builds dependency build scripts (including SQLx's and
//! libsqlite3-sys') possibly *before* this one, so a passing build script is not
//! acceptance. Final link inspection and the engine diagnostic are still required.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Settings that must be exactly these values. Anything else means an inherited
/// or overridden environment is being combined with our recipe.
const REQUIRED_SETTINGS: &[(&str, &str)] = &[
    ("SQLITE3_STATIC", "1"),
    ("SQLITE3_NO_PKG_CONFIG", "1"),
    ("LIBSQLITE3_SYS_USE_PKG_CONFIG", "0"),
];

// Release identity: changing it requires reviewing the native recipe and pins.
const SQLITE_VERSION: &str = "3.53.4";
const SQLITE_VERSION_NUMBER: &str = "3053004";
const SQLITE_SOURCE_ID: &str =
    "2026-07-24 19:02:57 bf7c7f30031888f4e796e429ab3978879485813aaca6f641c7b33e4e09459bcc";

fn fail(message: &str) -> ! {
    for variable in [
        "SQLITE3_INCLUDE_DIR",
        "SQLITE3_LIB_DIR",
        "SQLITE3_STATIC",
        "SQLITE3_NO_PKG_CONFIG",
        "LIBSQLITE3_SYS_USE_PKG_CONFIG",
        "LIBCLANG_PATH",
        "AVENCREW_SQLITE_VERSION",
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    panic!(
        "avencrew-store-sqlite native prerequisite guard: {message}\n\
         Hint: run `bash tools/bootstrap/build-sqlite.sh`, then build through\n\
         `tools/bootstrap/dev-env.sh` so these settings are exported."
    );
}

/// The project root, derived from this manifest's directory.
fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("store-sqlite must live at <project>/crates/store-sqlite")
        .to_path_buf()
}

/// Require an absolute path that stays inside the project.
fn require_project_relative(label: &str, raw: Option<String>, expected: &Path) -> PathBuf {
    let value = raw.unwrap_or_else(|| fail(&format!("{label} is not set")));
    let path = PathBuf::from(&value);
    if !path.is_absolute() {
        fail(&format!("{label} must be an absolute path, got {value}"));
    }
    let root = project_root()
        .canonicalize()
        .unwrap_or_else(|error| fail(&format!("cannot resolve project root: {error}")));
    let canonical = path.canonicalize().unwrap_or_else(|error| {
        fail(&format!(
            "{label} does not exist or cannot be resolved: {error}"
        ))
    });
    let selected = expected
        .canonicalize()
        .unwrap_or_else(|error| fail(&format!("selected native output is missing: {error}")));
    if !canonical.starts_with(&root) || canonical != selected {
        fail(&format!(
            "{label} must use the selected in-project target directory {expected:?}, got {canonical:?}"
        ));
    }
    canonical
}

/// Read `#define SQLITE_VERSION "x.y.z"` out of the selected header.
///
/// Tokenised rather than substring-matched: the header pads with tabs/multiple
/// spaces, and `SQLITE_VERSION_NUMBER` shares the `SQLITE_VERSION` prefix, so a
/// naive prefix test would either miss the real define or match the number one.
fn header_define(header: &Path, name: &str) -> Option<String> {
    let text = fs::read_to_string(header).ok()?;
    for line in text.lines() {
        let mut tokens = line.split_whitespace();
        if tokens.next() != Some("#define") || tokens.next() != Some(name) {
            continue;
        }
        let value = tokens.collect::<Vec<_>>().join(" ");
        if value.is_empty() {
            return None;
        }
        return Some(value.trim_matches('"').to_owned());
    }
    None
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=SQLITE3_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=SQLITE3_LIB_DIR");
    println!("cargo:rerun-if-env-changed=SQLITE3_STATIC");
    println!("cargo:rerun-if-env-changed=SQLITE3_NO_PKG_CONFIG");
    println!("cargo:rerun-if-env-changed=LIBSQLITE3_SYS_USE_PKG_CONFIG");
    println!("cargo:rerun-if-env-changed=LIBCLANG_PATH");
    println!("cargo:rerun-if-env-changed=AVENCREW_SQLITE_VERSION");
    println!("cargo:rerun-if-env-changed=TARGET");

    // 1. Fixed linkage settings. Reject rather than merge inherited overrides.
    for (name, expected) in REQUIRED_SETTINGS {
        match env::var(name) {
            Ok(actual) if actual == *expected => {}
            Ok(actual) => fail(&format!(
                "{name} must be {expected:?} for the controlled recipe, found {actual:?}. \
                 Clear any inherited SQLite override."
            )),
            Err(_) => fail(&format!("{name} must be set to {expected:?}")),
        }
    }

    // 2. Canonical in-project header and library paths.
    let target = env::var("TARGET").unwrap_or_else(|_| fail("Cargo TARGET is missing"));
    if !matches!(
        target.as_str(),
        "aarch64-apple-darwin" | "aarch64-unknown-linux-gnu"
    ) {
        fail("only the reviewed macOS ARM64 and native GNU/Linux ARM64 recipes are admitted");
    }
    let selected = project_root().join("build/native/sqlite").join(&target);
    let include_dir = require_project_relative(
        "SQLITE3_INCLUDE_DIR",
        env::var("SQLITE3_INCLUDE_DIR").ok(),
        &selected.join("include"),
    );
    let lib_dir = require_project_relative(
        "SQLITE3_LIB_DIR",
        env::var("SQLITE3_LIB_DIR").ok(),
        &selected.join("lib"),
    );

    // 3. The selected engine must actually be present.
    let header = include_dir.join("sqlite3.h");
    let library = lib_dir.join("libsqlite3.a");
    println!("cargo:rerun-if-changed={}", header.display());
    println!("cargo:rerun-if-changed={}", library.display());

    if !header.is_file() {
        fail(&format!(
            "selected header is missing: {}. Run tools/bootstrap/build-sqlite.sh.",
            header.display()
        ));
    }
    if !library.is_file() {
        fail(&format!(
            "static archive is missing: {}. Run tools/bootstrap/build-sqlite.sh.",
            library.display()
        ));
    }
    for (file, directory) in [(&header, &include_dir), (&library, &lib_dir)] {
        if file.canonicalize().ok().as_deref()
            != Some(directory.join(file.file_name().unwrap()).as_path())
        {
            fail("native header/archive must be real files in the selected directory, not alternate symlink targets");
        }
    }

    // 4. The header must be the pinned version, from this project's build.
    let expected_version = env::var("AVENCREW_SQLITE_VERSION").unwrap_or_else(|_| {
        fail("AVENCREW_SQLITE_VERSION is not set; it comes from tools/bootstrap/pins.json via dev-env.sh")
    });
    if expected_version != SQLITE_VERSION {
        fail("AVENCREW_SQLITE_VERSION does not match the reviewed release identity");
    }
    let actual_version = header_define(&header, "SQLITE_VERSION").unwrap_or_else(|| {
        fail(&format!(
            "could not read SQLITE_VERSION from {}",
            header.display()
        ))
    });
    if actual_version != expected_version {
        fail(&format!(
            "header identity mismatch: {} reports SQLite {actual_version}, pinned {expected_version}",
            header.display()
        ));
    }
    if header_define(&header, "SQLITE_VERSION_NUMBER").as_deref() != Some(SQLITE_VERSION_NUMBER)
        || header_define(&header, "SQLITE_SOURCE_ID").as_deref() != Some(SQLITE_SOURCE_ID)
    {
        fail("selected header version number/source ID does not match the reviewed amalgamation");
    }

    // 5. bindgen needs a real libclang, not merely a clang executable.
    let libclang_dir = PathBuf::from(
        env::var("LIBCLANG_PATH")
            .unwrap_or_else(|_| fail("LIBCLANG_PATH is not set; bindgen requires libclang")),
    );
    println!("cargo:rerun-if-changed={}", libclang_dir.display());
    let has_libclang = ["libclang.dylib", "libclang.so", "libclang.so.1"]
        .iter()
        .any(|name| libclang_dir.join(name).exists());
    if !has_libclang {
        fail(&format!(
            "no libclang in {}. bindgen cannot generate bindings; no arbitrary download fallback is permitted.",
            libclang_dir.display()
        ));
    }

    println!(
        "cargo:warning=avencrew-store-sqlite native guard OK: SQLite {actual_version}, header {}, archive {}",
        header.display(),
        library.display()
    );
}
