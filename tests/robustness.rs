// SPDX-License-Identifier: MIT OR Apache-2.0
//! Robustness tests: malformed protobuf, CRLF JSONL, binary-level checks
//! (pure --json output, --lang without a value) via CARGO_BIN_EXE.

use movara::backup::Backup;
use movara::protobuf::pb_replace;
use movara::spec::ReplaceSpec;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn tmpdir(tag: &str) -> PathBuf {
    let raw = std::env::temp_dir().join(format!("ap-rob-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&raw);
    fs::create_dir_all(&raw).unwrap();
    // canonicalize for macOS /tmp -> /private/tmp
    std::fs::canonicalize(&raw).unwrap_or(raw)
}

#[test]
fn protobuf_malformed_inputs_return_unchanged() {
    let spec = ReplaceSpec::new("/p/abc", "/p/cba").unwrap();
    // truncated key varint
    assert!(!pb_replace(&[0xff], &spec).1);
    // length varint = u64::MAX (must not overflow/panic)
    let mut huge = vec![0x0a];
    huge.extend_from_slice(&[0xff; 9]);
    huge.push(0x01);
    let (out, ch) = pb_replace(&huge, &spec);
    assert!(!ch);
    assert_eq!(out, huge);
    // declared length larger than the buffer
    let short = vec![0x0a, 0x10, b'a', b'b'];
    let (out, ch) = pb_replace(&short, &spec);
    assert!(!ch);
    assert_eq!(out, short);
    // wire type 3 (groups) and 4 (end-group): unsupported -> unchanged
    let group = vec![0x0b, 0x0c];
    let (out, ch) = pb_replace(&group, &spec);
    assert!(!ch);
    assert_eq!(out, group);
    // truncated fixed64 / fixed32
    assert!(!pb_replace(&[0x09, 0x01, 0x02], &spec).1);
    assert!(!pb_replace(&[0x0d, 0x01], &spec).1);
    // deep nesting beyond the cap: unchanged, no stack overflow
    let mut deep = Vec::new();
    for _ in 0..200 {
        deep.extend_from_slice(&[0x0a, 0x02]); // field 1, len 2...
    }
    // ...wrapping two literal bytes at the innermost level
    deep.extend_from_slice(b"zz");
    let (out, ch) = pb_replace(&deep, &spec);
    assert!(!ch);
    assert_eq!(out, deep);
}

#[test]
fn jsonl_crlf_line_endings_preserved() {
    let dir = tmpdir("crlf");
    let f = dir.join("s.jsonl");
    let old = "/p/abc";
    fs::write(&f, format!("{{\"cwd\":\"{}\"}}\r\n", old)).unwrap();
    let spec = ReplaceSpec::new(old, "/p/cba").unwrap();
    let mut backup = Backup::new(&dir.join("bk"), &spec, vec![], false);
    assert!(movara::rewriters::rewrite_jsonl_file(&f, &spec, &mut backup, false).unwrap());
    let out = fs::read_to_string(&f).unwrap();
    assert_eq!(out, "{\"cwd\":\"/p/cba\"}\r\n");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn jsonl_invalid_utf8_left_untouched() {
    let dir = tmpdir("utf8");
    let f = dir.join("s.jsonl");
    // invalid UTF-8 byte inside the line
    fs::write(&f, b"{\"cwd\":\"/p/abc\",\xff}\n").unwrap();
    let spec = ReplaceSpec::new("/p/abc", "/p/cba").unwrap();
    let mut backup = Backup::new(&dir.join("bk"), &spec, vec![], false);
    assert!(!movara::rewriters::rewrite_jsonl_file(&f, &spec, &mut backup, false).unwrap());
    assert_eq!(fs::read(&f).unwrap(), b"{\"cwd\":\"/p/abc\",\xff}\n");
    let _ = fs::remove_dir_all(&dir);
}

// ------------------------------------------------- binary-level checks
// These spawn the compiled binary via std::process::Command with .arg()
// elements — the Rust equivalent of subprocess.run([...], shell=False).
// No shell is ever involved: arguments (all literals or cargo-injected
// CARGO_BIN_EXE paths and test tempdirs) are passed as an argv array, so
// there is no command-string construction and no injection surface.

fn bin_path() -> &'static str {
    // CARGO_BIN_EXE_movara is set for integration tests by cargo
    env!("CARGO_BIN_EXE_movara")
}

#[test]
fn json_modes_emit_pure_json_on_stdout() {
    let home = tmpdir("json-home");
    // agents --json
    let out = Command::new(bin_path())
        .arg("--lang")
        .arg("en")
        .arg("agents")
        .arg("--json")
        .env("MOVARA_HOME", &home)
        .output()
        .unwrap();
    assert!(out.status.success());
    serde_json::from_slice::<serde_json::Value>(&out.stdout).expect("agents --json stdout");

    // scan --json
    let out = Command::new(bin_path())
        .arg("scan")
        .arg("--json")
        .arg("--from")
        .arg("/tmp/definitely-not-here")
        .env("MOVARA_HOME", &home)
        .output()
        .unwrap();
    assert!(out.status.success());
    serde_json::from_slice::<serde_json::Value>(&out.stdout).expect("scan --json stdout");

    // migrate --json (nothing to change is fine; stdout must be ONE json doc)
    let out = Command::new(bin_path())
        .arg("migrate")
        .arg("--json")
        .arg("--from")
        .arg("/tmp/old-json-test")
        .arg("--to")
        .arg("/tmp/new-json-test")
        .arg("--yes")
        .arg("--backup-dir")
        .arg(home.join("bk"))
        .env("MOVARA_HOME", &home)
        .output()
        .unwrap();
    assert!(out.status.success());
    serde_json::from_slice::<serde_json::Value>(&out.stdout).expect("migrate --json stdout");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn movara_lang_without_value_is_a_usage_error_not_a_panic() {
    let home = tmpdir("langmv");
    let out = Command::new(bin_path())
        .arg("--lang")
        .env("MOVARA_HOME", &home)
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    // any clap usage error is fine; the point is no panic
    assert!(
        stderr.contains("Usage")
            || stderr.contains("usage")
            || stderr.contains("requires")
            || stderr.contains("--lang"),
        "expected a clap usage error, got: {}",
        stderr
    );
    assert!(!stderr.contains("panicked"), "must not panic: {}", stderr);
    let _ = fs::remove_dir_all(&home);
}
