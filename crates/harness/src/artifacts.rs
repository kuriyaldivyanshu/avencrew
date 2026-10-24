//! Deterministic format checks; passing syntax never satisfies business review.
use avencrew_contracts::{
    runtime::{content_digest, ArtifactFormat, ArtifactValidation, CheckStatus},
    scalars::Counter,
};
use std::collections::BTreeMap;

pub fn validate(
    format: ArtifactFormat,
    bytes: &[u8],
    patch_bases: &BTreeMap<String, String>,
) -> ArtifactValidation {
    let mut report = ArtifactValidation {
        schema_version: "avencrew.artifact-validation/1".into(),
        format,
        sha256: content_digest(bytes),
        byte_size: Counter::new(bytes.len().to_string()).expect("bounded process address space"),
        status: CheckStatus::Passed,
        codes: vec![],
        base_sha256: None,
        result_sha256: None,
    };
    let verdict = if bytes.len() > 262_144 {
        Err("byte_limit")
    } else if bytes.is_empty() {
        Err("empty_output")
    } else {
        match std::str::from_utf8(bytes) {
            Err(_) => Err("invalid_utf8"),
            Ok(text) if text.contains('\0') => Err("nul_byte"),
            Ok(text) => match format {
                ArtifactFormat::Text => Ok("nonempty_utf8"),
                ArtifactFormat::Json => avencrew_contracts::decode_json(bytes)
                    .map(|_| "strict_json")
                    .map_err(|_| "invalid_or_duplicate_json"),
                ArtifactFormat::Csv => check_csv(bytes).map(|_| "rectangular_csv"),
                ArtifactFormat::Patch => match apply_patch(text, patch_bases) {
                    Ok((base, result)) => {
                        report.base_sha256 = Some(content_digest(base.as_bytes()));
                        report.result_sha256 = Some(content_digest(result.as_bytes()));
                        Ok("applies_to_exact_supplied_base")
                    }
                    Err("missing_patch_base") => {
                        report.status = CheckStatus::Unknown;
                        Err("missing_patch_base")
                    }
                    Err(code) => Err(code),
                },
            },
        }
    };
    match verdict {
        Ok(code) => report.codes.push(code.into()),
        Err(code) => {
            if report.status != CheckStatus::Unknown {
                report.status = CheckStatus::Failed;
            }
            report.codes.push(code.into());
        }
    }
    report
}

fn check_csv(bytes: &[u8]) -> Result<(), &'static str> {
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        Start,
        Plain,
        Quoted,
        Closed,
    }
    let (mut state, mut columns, mut width, mut rows, mut cells, mut started) =
        (State::Start, 1usize, None, 0usize, 0usize, false);
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if state == State::Quoted {
            if c == b'"' {
                state = State::Closed;
            }
            started = true;
            i += 1;
            continue;
        }
        if state == State::Closed && c == b'"' {
            state = State::Quoted;
            i += 1;
            continue;
        }
        match c {
            b'"' if state == State::Start => {
                state = State::Quoted;
                started = true;
            }
            b'"' => return Err("invalid_csv_quote"),
            b',' => {
                columns += 1;
                cells += 1;
                state = State::Start;
                started = true;
            }
            b'\n' | b'\r' => {
                if c == b'\r' {
                    if bytes.get(i + 1) != Some(&b'\n') {
                        return Err("invalid_csv_line_end");
                    }
                    i += 1;
                }
                if width.is_some_and(|n| n != columns) {
                    return Err("unequal_csv_columns");
                }
                width = Some(columns);
                rows += 1;
                cells += 1;
                columns = 1;
                state = State::Start;
                started = false;
            }
            _ if state == State::Closed => return Err("invalid_csv_after_quote"),
            _ => {
                state = State::Plain;
                started = true;
            }
        }
        if columns > 128 || rows > 10000 || cells > 100000 {
            return Err("csv_shape_limit");
        }
        i += 1;
    }
    if state == State::Quoted {
        return Err("unclosed_csv_quote");
    }
    if started && width.is_some_and(|n| n != columns) {
        return Err("unequal_csv_columns");
    }
    Ok(())
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', '\0', ':'])
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}
fn range(text: &str, prefix: char) -> Result<(usize, usize), &'static str> {
    let text = text.strip_prefix(prefix).ok_or("invalid_patch_header")?;
    let (start, count) = text.split_once(',').unwrap_or((text, "1"));
    let start = start.parse::<usize>().map_err(|_| "invalid_patch_header")?;
    let count = count.parse::<usize>().map_err(|_| "invalid_patch_header")?;
    if start > 1_000_000 || count > 1_000_000 {
        return Err("patch_range_limit");
    }
    Ok((start, count))
}
fn apply_patch<'a>(
    patch: &str,
    bases: &'a BTreeMap<String, String>,
) -> Result<(&'a str, String), &'static str> {
    if !patch.ends_with('\n') || patch.contains('\r') {
        return Err("unsupported_patch_line_end");
    }
    let lines = patch.lines().collect::<Vec<_>>();
    let mut p = 0;
    while lines
        .get(p)
        .is_some_and(|l| l.starts_with("diff --git ") || l.starts_with("index "))
    {
        p += 1;
    }
    let old = lines
        .get(p)
        .and_then(|l| l.strip_prefix("--- a/"))
        .ok_or("unsupported_patch_header")?;
    let new = lines
        .get(p + 1)
        .and_then(|l| l.strip_prefix("+++ b/"))
        .ok_or("unsupported_patch_header")?;
    if old != new || !safe_path(old) {
        return Err("unsafe_or_renamed_patch_target");
    }
    let base = bases.get(old).ok_or("missing_patch_base")?;
    if !base.is_empty() && !base.ends_with('\n') {
        return Err("unsupported_base_line_end");
    }
    let source = base.lines().collect::<Vec<_>>();
    let (mut cursor, mut output, mut hunks) = (0usize, Vec::new(), 0usize);
    p += 2;
    while p < lines.len() {
        let header = lines[p]
            .strip_prefix("@@ ")
            .and_then(|h| h.split_once(" @@"))
            .map(|(h, _)| h)
            .ok_or("invalid_patch_hunk")?;
        let (old_range, new_range) = header.split_once(' ').ok_or("invalid_patch_hunk")?;
        let (old_start, old_count) = range(old_range, '-')?;
        let (new_start, new_count) = range(new_range, '+')?;
        let start = if old_start == 0 { 0 } else { old_start - 1 };
        if start < cursor || start > source.len() {
            return Err("overlapping_patch_hunks");
        }
        output.extend_from_slice(&source[cursor..start]);
        cursor = start;
        let expected_new = if new_count == 0 {
            output.len()
        } else {
            output.len() + 1
        };
        if new_start != expected_new {
            return Err("invalid_patch_new_range");
        }
        p += 1;
        let (mut removed, mut added) = (0usize, 0usize);
        while p < lines.len() && !lines[p].starts_with("@@ ") {
            let line = lines[p];
            let kind = line
                .as_bytes()
                .first()
                .copied()
                .ok_or("invalid_patch_line")?;
            let text = line.get(1..).ok_or("invalid_patch_line")?;
            match kind {
                b' ' => {
                    if source.get(cursor) != Some(&text) {
                        return Err("patch_context_mismatch");
                    }
                    output.push(text);
                    cursor += 1;
                    removed += 1;
                    added += 1;
                }
                b'-' => {
                    if source.get(cursor) != Some(&text) {
                        return Err("patch_context_mismatch");
                    }
                    cursor += 1;
                    removed += 1;
                }
                b'+' => {
                    output.push(text);
                    added += 1;
                }
                _ => return Err("unsupported_patch_line"),
            }
            if removed > old_count || added > new_count {
                return Err("patch_hunk_count_mismatch");
            }
            p += 1;
        }
        if removed != old_count || added != new_count {
            return Err("patch_hunk_count_mismatch");
        }
        hunks += 1;
    }
    if hunks == 0 {
        return Err("missing_patch_hunk");
    }
    output.extend_from_slice(&source[cursor..]);
    let result = if output.is_empty() {
        String::new()
    } else {
        output.join("\n") + "\n"
    };
    if result.len() > 262144 {
        return Err("patch_result_limit");
    }
    Ok((base, result))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structured_formats_reject_false_or_incomplete_validation() {
        let bases = BTreeMap::new();
        assert_eq!(
            validate(ArtifactFormat::Json, br#"{"a":1,"a":2}"#, &bases).status,
            CheckStatus::Failed
        );
        assert_eq!(
            validate(ArtifactFormat::Json, br#"{"a":1.25}"#, &bases).status,
            CheckStatus::Passed
        );
        for bytes in [
            b"a,b\n1\n".as_slice(),
            b"a,b\n\"unfinished,b\n",
            b"a,b\n\"x\"bad,y\n",
        ] {
            assert_eq!(
                validate(ArtifactFormat::Csv, bytes, &bases).status,
                CheckStatus::Failed
            );
        }
        assert_eq!(
            validate(
                ArtifactFormat::Csv,
                b"a,b\r\n\"x,y\",\"a\"\"b\"\r\n",
                &bases
            )
            .status,
            CheckStatus::Passed
        );
        assert_eq!(
            validate(ArtifactFormat::Text, b"", &bases).status,
            CheckStatus::Failed
        );
    }
    #[test]
    fn patch_validation_requires_exact_safe_base_and_real_hunk_counts() {
        let bases = BTreeMap::from([("src/a.txt".into(), "old\nkeep\n".into())]);
        let patch = b"--- a/src/a.txt\n+++ b/src/a.txt\n@@ -1,2 +1,2 @@\n-old\n+new\n keep\n";
        let report = validate(ArtifactFormat::Patch, patch, &bases);
        assert_eq!(report.status, CheckStatus::Passed);
        assert_eq!(report.result_sha256, Some(content_digest(b"new\nkeep\n")));
        assert_eq!(
            validate(ArtifactFormat::Patch, patch, &BTreeMap::new()).status,
            CheckStatus::Unknown
        );
        let bad = String::from_utf8(patch.to_vec())
            .unwrap()
            .replace("-old", "-wrong");
        assert_eq!(
            validate(ArtifactFormat::Patch, bad.as_bytes(), &bases).status,
            CheckStatus::Failed
        );
        let bad = String::from_utf8(patch.to_vec())
            .unwrap()
            .replace("src/a.txt", "../a.txt");
        assert_eq!(
            validate(ArtifactFormat::Patch, bad.as_bytes(), &bases).status,
            CheckStatus::Failed
        );
    }
}
