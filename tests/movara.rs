// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests for the one-shot `movara mv` command (move + history migration).

mod common;

use common::Fixture;
use movara::backup;
use std::fs;
use std::path::PathBuf;

fn mv_core(fx: &Fixture, src: &str, dst: &str, deep: bool) -> anyhow::Result<PathBuf> {
    let backup_dir = fx.tmp.join("backups");
    // in-process equivalent of `movara mv --yes`
    let ctx = &fx.ctx;
    let (src_abs, new_abs) =
        movara::cli::mv_resolve(std::path::Path::new(src), std::path::Path::new(dst))?;
    let spec =
        movara::spec::ReplaceSpec::new(&src_abs.to_string_lossy(), &new_abs.to_string_lossy())?;
    let adapters = movara::adapters::get_adapters(None)?;
    let mut bk = backup::Backup::new(
        &backup_dir,
        &spec,
        adapters.iter().map(|a| a.name().to_string()).collect(),
        false,
    );
    bk.manifest.moved_project = Some((spec.old.clone(), spec.new.clone()));
    movara::cli::move_dir(
        std::path::Path::new(&spec.old),
        std::path::Path::new(&spec.new),
    )?;
    for a in &adapters {
        if a.installed(ctx) {
            a.migrate(ctx, &spec, &mut bk, deep)?;
        }
    }
    bk.save()?;
    Ok(backup_dir)
}

#[test]
fn rename_in_place_moves_dir_and_migrates_state() {
    let fx = Fixture::new("mv-rename");
    let backup_dir = mv_core(&fx, &fx.old, &fx.new, false).unwrap();
    assert!(fs::metadata(&fx.new).is_ok());
    assert!(!PathBuf::from(&fx.old).exists());
    // claude bucket renamed to the new encoding
    let enc = movara::encodings::dash_encode(&fx.new);
    assert!(fx.ctx.h(".claude/projects").join(enc).is_dir());
    // opencode db updated
    let con = rusqlite::Connection::open(fx.ctx.d("opencode/opencode.db")).unwrap();
    let wt: String = con
        .query_row("SELECT worktree FROM project", [], |r| r.get(0))
        .unwrap();
    assert_eq!(wt, fx.new);
    // undo restores the directory and the state
    let ids = fs::read_dir(&backup_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("manifest.json").is_file())
        .collect::<Vec<_>>();
    backup::undo(&backup_dir, ids[0].file_name().unwrap().to_str().unwrap()).unwrap();
    assert!(PathBuf::from(&fx.old).is_dir());
    assert!(!PathBuf::from(&fx.new).exists());
    let enc_old = movara::encodings::dash_encode(&fx.old);
    assert!(fx.ctx.h(".claude/projects").join(enc_old).is_dir());
}

#[test]
fn move_into_existing_directory() {
    let fx = Fixture::new("mv-into");
    let dest = fx.tmp.join("dest");
    fs::create_dir_all(&dest).unwrap();
    mv_core(&fx, &fx.old, dest.to_str().unwrap(), false).unwrap();
    assert!(dest.join("abc").is_dir());
}

#[test]
fn refuses_bad_targets() {
    let fx = Fixture::new("mv-refuse");
    let taken = fx.tmp.join("taken");
    fs::create_dir_all(taken.join("abc")).unwrap();
    assert!(mv_core(&fx, &fx.old, taken.to_str().unwrap(), false).is_err());
    let missing = fx.tmp.join("nope").join("abc");
    assert!(mv_core(&fx, &fx.old, missing.to_str().unwrap(), false).is_err());
}
