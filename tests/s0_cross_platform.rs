// SPDX-License-Identifier: MIT OR Apache-2.0
//! S0 cross-platform layer: format-2 root-kind members, portable
// (slash + NFC) rels, four-form needle variants, BOM tolerance,
//! reserved-name escape, and the format-1 legacy read path.

mod common;

use common::Fixture;
use movara::archive::{self, ExportOpts};
use movara::ctx::{to_portable_rel, RootKind};
use movara::spec::ReplaceSpec;
use std::fs;

fn s0_target_ctx(tag: &str) -> (movara::ctx::Ctx, std::path::PathBuf) {
    let raw = std::env::temp_dir().join(format!("movara-s0-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&raw);
    fs::create_dir_all(&raw).unwrap();
    let tmp = movara::ctx::de_verbatim(&fs::canonicalize(&raw).unwrap_or(raw));
    let ctx = movara::ctx::Ctx {
        config_home: tmp.join(".config"),
        data_home: tmp.join(".local").join("share"),
        home: tmp.clone(),
    };
    (ctx, tmp)
}

#[test]
fn portable_rel_slashes_and_nfc() {
    // backslash -> forward slash
    assert_eq!(
        to_portable_rel(r".config\Cursor\User"),
        ".config/Cursor/User"
    );
    // decomposed e+combining-acute composes to precomposed é
    let decomposed = "proj\u{65}\u{301}ct";
    let composed = "proj\u{e9}ct";
    assert_eq!(to_portable_rel(decomposed), composed);
    assert_eq!(to_portable_rel(composed), composed, "NFC is idempotent");
}

#[test]
fn root_kind_segments_and_segments_roundtrip() {
    assert_eq!(RootKind::Home.segment(), "home");
    assert_eq!(RootKind::Config.segment(), "config");
    assert_eq!(RootKind::Data.segment(), "data");
    assert_eq!(RootKind::DataLocal.segment(), "datalocal");
    for k in [
        RootKind::Home,
        RootKind::Config,
        RootKind::Data,
        RootKind::DataLocal,
    ] {
        assert_eq!(RootKind::from_segment(k.segment()), Some(k));
    }
    assert_eq!(RootKind::from_segment("bogus"), None);
}

#[test]
fn export_writes_format2_with_kind_segments() {
    let fx = Fixture::new("s0-fmt");
    let out = fx.tmp.join("fmt.tar.gz");
    let writer = archive::ArchiveWriter::create(&out).unwrap();
    let opts = ExportOpts {
        out: out.clone(),
        filtered: false,
        paths: vec![],
        project: None,
        state_only: false,
    };
    let list = movara::adapters::get_adapters(None).unwrap();
    let report = archive::run_export_into(&fx.ctx, &list, &opts, writer, None).unwrap();
    // Home-direct agents carry the home segment
    assert!(report
        .members
        .iter()
        .any(|m| m.starts_with("data/claude/home/.claude/")));
    // Data-rooted agents carry the data segment
    assert!(report
        .members
        .iter()
        .any(|m| m.starts_with("data/opencode/data/opencode/")));
    let staging = archive::open(&out).unwrap();
    assert_eq!(staging.manifest.format, archive::FORMAT);
    assert_eq!(staging.manifest.format, 2);
    let _ = fs::remove_dir_all(&fx.tmp);
}

#[test]
fn format1_archive_reads_as_legacy() {
    // hand-craft a format-1 archive (home-relative members, no kind
    // segment): must still import — same-OS placement is identity
    let fx = Fixture::new("s0-leg");
    let (ctx_b, tmp_b) = s0_target_ctx("s0-leg");
    let arch = tmp_b.join("legacy.tar.gz");
    {
        let mut w = archive::ArchiveWriter::create(&arch).unwrap();
        // NOTE: format-1 members are home-relative: data/claude/.claude/...
        w.add_dir("data/claude/.claude/projects/leg").unwrap();
        w.add_file(
            "data/claude/.claude/projects/leg/s.jsonl",
            b"{\"cwd\":\"/nowhere\"}\n",
        )
        .unwrap();
        let manifest = archive::ArchiveManifest {
            format: archive::LEGACY_FORMAT,
            created: String::new(),
            host: "t".into(),
            os: std::env::consts::OS.into(),
            source_home: "/home/user".into(),
            movara_version: movara::VERSION.into(),
            agents: vec!["claude".into()],
            filtered: true,
            project: None,
            paths: vec![],
            stats: Default::default(),
        };
        w.finish(&manifest).unwrap();
    }
    // direct import (no project semantics): legacy placement is
    // home-relative identity on os-match
    let staging = archive::open(&arch).unwrap();
    assert_eq!(staging.manifest.format, 1);
    let list = movara::adapters::get_adapters(None).unwrap();
    let mut bk =
        movara::backup::Backup::new(&tmp_b.join("bk"), &ReplaceSpec::identity(), vec![], false);
    let rep = archive::run_import(
        &ctx_b,
        &staging,
        &list,
        &archive::ImportOpts {
            rules: vec![],
            agents: None,
            policy: archive::Policy::Skip,
            allow_missing: true,
            dry_run: false,
        },
        &mut bk,
    )
    .unwrap();
    assert!(rep.placed >= 1, "legacy member not placed");
    assert!(ctx_b.h(".claude/projects/leg/s.jsonl").is_file());
    let _ = fs::remove_dir_all(&tmp_b);
    let _ = fs::remove_dir_all(&fx.tmp);
}

#[test]
fn four_form_needle_variants_rewrite() {
    // POSIX-only rule: no variants (a literal /c/... dir must not match)
    let spec = ReplaceSpec::new("/p/abc", "/p/cba").unwrap();
    assert_eq!(spec.replace("see /c/Users/x/abc"), "see /c/Users/x/abc");

    // cross-separator rule: all four forms rekey into the target raw form
    let spec = ReplaceSpec::new(r"C:\Users\a\abc", "/home/b/abc").unwrap();
    assert_eq!(
        spec.replace(r"raw C:\Users\a\abc tail"),
        "raw /home/b/abc tail"
    );
    assert_eq!(
        spec.replace(r"fwd C:/Users/a/abc tail"),
        "fwd /home/b/abc tail"
    );
    assert_eq!(
        spec.replace(r"esc C:\\Users\\a\\abc tail"),
        "esc /home/b/abc tail"
    );
    assert_eq!(
        spec.replace(r"msys /c/Users/a/abc tail"),
        "msys /home/b/abc tail"
    );

    // same-style (Windows↔Windows): each form keeps its own form
    let spec = ReplaceSpec::new(r"C:\Users\a\abc", r"D:\Users\b\abc").unwrap();
    let out = spec.replace(r"raw C:\Users\a\abc tail");
    assert_eq!(out, r"raw D:\Users\b\abc tail");
    let out = spec.replace(r"esc C:\\Users\\a\\abc tail");
    assert_eq!(
        out, r"esc D:\\Users\\b\\abc tail",
        "JSON-escaped must stay escaped"
    );
}

#[test]
fn bom_prefixed_json_is_rewritten_not_skipped() {
    let dir = std::env::temp_dir().join(format!("s0-bom-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let f = dir.join("s.json");
    // UTF-8 BOM + a JSON body carrying the path
    let mut body = vec![0xEF, 0xBB, 0xBF];
    body.extend_from_slice(b"{\"cwd\":\"/p/abc\"}\n");
    fs::write(&f, body).unwrap();
    let spec = ReplaceSpec::new("/p/abc", "/p/cba").unwrap();
    let mut bk = movara::backup::Backup::new(&dir.join("bk"), &spec, vec![], false);
    let changed = movara::rewriters::rewrite_json_file(&f, &spec, &mut bk).unwrap();
    assert!(changed, "BOM-prefixed JSON must not be silently skipped");
    let out = fs::read(&f).unwrap();
    // the S0 contract: parse succeeds and the path is rewritten (BOM may
    // legitimately drop on reserialization — byte-preservation of the
    // BOM itself is not required, only non-skip)
    assert!(out.windows(4).any(|w| w == b"cba\""));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn reserved_name_escaped_at_placement() {
    // a member whose filename is a Windows reserved device name:
    // placement escapes it to _x_<name> instead of failing
    let fx = Fixture::new("s0-res");
    let (ctx_b, tmp_b) = s0_target_ctx("s0-res");
    let arch = tmp_b.join("res.tar.gz");
    {
        let mut w = archive::ArchiveWriter::create(&arch).unwrap();
        w.add_dir("data/claude/home/.claude/projects/x").unwrap();
        w.add_file(
            "data/claude/home/.claude/projects/x/nul.jsonl",
            b"{\"cwd\":\"/nowhere\"}\n",
        )
        .unwrap();
        let manifest = archive::ArchiveManifest {
            format: archive::FORMAT,
            created: String::new(),
            host: "t".into(),
            os: std::env::consts::OS.into(),
            source_home: "/home/user".into(),
            movara_version: movara::VERSION.into(),
            agents: vec!["claude".into()],
            filtered: true,
            project: None,
            paths: vec![],
            stats: Default::default(),
        };
        w.finish(&manifest).unwrap();
    }
    let staging = archive::open(&arch).unwrap();
    let list = movara::adapters::get_adapters(None).unwrap();
    let mut bk =
        movara::backup::Backup::new(&tmp_b.join("bk"), &ReplaceSpec::identity(), vec![], false);
    archive::run_import(
        &ctx_b,
        &staging,
        &list,
        &archive::ImportOpts {
            rules: vec![],
            agents: None,
            policy: archive::Policy::Skip,
            allow_missing: true,
            dry_run: false,
        },
        &mut bk,
    )
    .unwrap();
    // the escaped name lands (on Linux `nul.jsonl` is creatable, but the
    // guard fires unconditionally so the behavior is uniform cross-OS)
    assert!(
        ctx_b.h(".claude/projects/x/_x_nul.jsonl").is_file(),
        "reserved name was not escaped"
    );
    let _ = fs::remove_dir_all(&tmp_b);
    let _ = fs::remove_dir_all(&fx.tmp);
}
