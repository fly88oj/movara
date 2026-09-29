// SPDX-License-Identifier: MIT OR Apache-2.0
//! Remaining adapters: continue, pi, droid (Factory), crush, aider,
//! cc-connect, and the generic --extra-root rewriter.

use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::encodings;
use crate::rewriters;
use crate::spec::ReplaceSpec;
use crate::sqlite;
use anyhow::Result;
use std::path::PathBuf;

// ============================================================ continue

pub struct ContinueAdapter;

impl ContinueAdapter {
    fn index_db(&self, ctx: &Ctx) -> PathBuf {
        ctx.h(".continue/index/index.sqlite")
    }

    fn roots(&self, ctx: &Ctx) -> Vec<PathBuf> {
        [
            ".continue/sessions",
            ".continue/index/globalContext.json",
            ".continue/config.yaml",
            ".continue/config.json",
        ]
        .iter()
        .map(|rel| ctx.h(rel))
        .filter(|p| p.exists())
        .collect()
    }
}

impl Adapter for ContinueAdapter {
    fn name(&self) -> &'static str {
        "continue"
    }
    fn display(&self) -> &'static str {
        "Continue"
    }
    fn note(&self) -> &'static str {
        "~/.continue/sessions/*.json workspaceDirectory (file:// URI), \
         sessions/sessions.json, index/index.sqlite tag_catalog.dir"
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["continue"]
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.h(".continue")]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = self.scan_tree(spec, &self.roots(ctx));
        let db = self.index_db(ctx);
        if db.is_file() {
            if let Ok(con) = sqlite::open_ro(&db) {
                let n = super::sqlite_like_count(
                    &con,
                    "SELECT \"dir\" FROM \"tag_catalog\" \
                     WHERE \"dir\" LIKE ?",
                    &spec.like_pattern(),
                ) + super::sqlite_like_count(
                    &con,
                    "SELECT rowid FROM \"global_cache\" WHERE \"dir\" LIKE ?",
                    &spec.like_pattern(),
                );
                if n > 0 {
                    out.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: format!("{}::tag_catalog/global_cache.dir", db.display()),
                        detail: format!("{} rows", n),
                    });
                }
            }
        }
        out
    }

    fn migrate(
        &self,
        ctx: &Ctx,
        spec: &ReplaceSpec,
        backup: &mut Backup,
        deep: bool,
    ) -> Result<Vec<Finding>> {
        let mut actions = self.migrate_text_tree(spec, backup, &self.roots(ctx), deep)?;
        let db = self.index_db(ctx);
        if db.is_file() {
            let pat = spec.like_pattern();
            backup.record_db(&db)?;
            if !backup.dry_run {
                let con = sqlite::open_rw(&db)?;
                // tag_catalog is keyed by (dir, branch, artifactId, path);
                // update via rowid, matching all rows that share the old dir
                let rows: Vec<(i64, String)> = {
                    let mut stmt = con.prepare(
                        "SELECT rowid,\"dir\" FROM \"tag_catalog\" \
                         WHERE \"dir\" LIKE ?",
                    )?;
                    let it = stmt.query_map([&pat], |r| Ok((r.get(0)?, r.get(1)?)))?;
                    it.filter_map(|x| x.ok()).collect()
                };
                let mut total = 0;
                for (rowid, dir) in rows {
                    let new_dir = spec.replace(&dir);
                    if new_dir != dir {
                        con.execute(
                            "UPDATE \"tag_catalog\" SET \"dir\"=? \
                             WHERE rowid=?",
                            rusqlite::params![new_dir, rowid],
                        )?;
                        total += 1;
                    }
                }
                if total > 0 {
                    // global_cache.dir mirrors tag_catalog.dir (the
                    // same workspace URIs)
                    let _ = super::rewrite_pair(
                        &con,
                        &spec.like_patterns(),
                        spec,
                        "SELECT rowid,\"dir\" FROM \"global_cache\" \
                         WHERE \"dir\" LIKE ?",
                        "UPDATE \"global_cache\" SET \"dir\"=? WHERE rowid=?",
                    );
                    actions.push(mk(self.name(), "sqlite", &db, "tag_catalog rows updated"));
                }
            }
        }
        Ok(actions)
    }
}

// ============================================================ pi

pub struct PiAdapter;

impl PiAdapter {
    fn sessions(&self, ctx: &Ctx) -> PathBuf {
        ctx.h(".pi/agent/sessions")
    }

    fn memory(&self, ctx: &Ctx) -> PathBuf {
        ctx.h(".pi/agent/projects-memory")
    }
}

impl Adapter for PiAdapter {
    fn name(&self) -> &'static str {
        "pi"
    }
    fn display(&self) -> &'static str {
        "pi coding agent / gsd"
    }
    fn note(&self) -> &'static str {
        "~/.pi/agent/sessions/--<encoded-cwd>--/ (header cwd), \
         projects-memory/<basename>/ (3rd-party memory ext), \
         run-history.jsonl, settings.json"
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["pi"]
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.h(".pi/agent")]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let (old_enc, new_enc) = (
            encodings::pi_bucket(&spec.old),
            encodings::pi_bucket(&spec.new),
        );
        let mut out =
            super::encoded_bucket_findings(self.name(), &self.sessions(ctx), &old_enc, &new_enc);
        let mem = self.memory(ctx);
        let (old_base, new_base) = (
            encodings::basename(&spec.old),
            encodings::basename(&spec.new),
        );
        let d = mem.join(&old_base);
        if d.is_dir() && old_base != new_base {
            out.push(Finding {
                agent: self.name().into(),
                kind: "dir_rename".into(),
                target: d.to_string_lossy().into_owned(),
                detail: "(memory ext)".into(),
            });
        }
        let roots: Vec<PathBuf> = [
            self.sessions(ctx),
            mem,
            ctx.h(".pi/agent/run-history.jsonl"),
            ctx.h(".pi/agent/settings.json"),
        ]
        .into_iter()
        .filter(|p| p.exists())
        .collect();
        out.extend(self.scan_tree(spec, &roots));
        out
    }

    fn migrate(
        &self,
        ctx: &Ctx,
        spec: &ReplaceSpec,
        backup: &mut Backup,
        deep: bool,
    ) -> Result<Vec<Finding>> {
        let mut actions = Vec::new();
        let (old_enc, new_enc) = (
            encodings::pi_bucket(&spec.old),
            encodings::pi_bucket(&spec.new),
        );
        for (o, n) in
            super::rename_encoded_children(&self.sessions(ctx), &old_enc, &new_enc, backup)
        {
            actions.push(mk(
                self.name(),
                "dir_rename",
                &o,
                &format!("-> {}", n.display()),
            ));
        }
        let mem = self.memory(ctx);
        let (old_base, new_base) = (
            encodings::basename(&spec.old),
            encodings::basename(&spec.new),
        );
        let d = mem.join(&old_base);
        let new_d = mem.join(&new_base);
        if super::rename_dir(&d, &new_d, backup) {
            actions.push(mk(
                self.name(),
                "dir_rename",
                &d,
                &format!("-> {}", new_d.display()),
            ));
        }
        // run-history lines are records: the cwd identity field always,
        // task text only with --deep; settings.json is config, always text
        let rh = ctx.h(".pi/agent/run-history.jsonl");
        if rh.is_file() && rewriters::rewrite_jsonl_file(&rh, spec, backup, deep)? {
            actions.push(mk(self.name(), "file", &rh, "jsonl"));
        }
        let st = ctx.h(".pi/agent/settings.json");
        if st.is_file() && rewriters::rewrite_text_file(&st, spec, backup)? {
            actions.push(mk(self.name(), "file", &st, "text"));
        }
        let roots: Vec<PathBuf> = [self.sessions(ctx), mem]
            .into_iter()
            .filter(|p| p.exists())
            .collect();
        actions.extend(self.migrate_text_tree(spec, backup, &roots, deep)?);
        Ok(actions)
    }
}

// ============================================================ droid

pub struct DroidAdapter;

impl Adapter for DroidAdapter {
    fn name(&self) -> &'static str {
        "droid"
    }
    fn display(&self) -> &'static str {
        "Factory Droid / agent"
    }
    fn note(&self) -> &'static str {
        "~/.factory/sessions/<encoded>/ where encoding = realpath with \
         only slashes turned into dashes (dots/underscores kept)"
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["droid"]
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.h(".factory")]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let (old_enc, new_enc) = (
            encodings::droid_bucket(&spec.old),
            encodings::droid_bucket(&spec.new),
        );
        let mut out = super::encoded_bucket_findings(
            self.name(),
            &ctx.h(".factory/sessions"),
            &old_enc,
            &new_enc,
        );
        out.extend(self.scan_tree(spec, &[ctx.h(".factory")]));
        out
    }

    fn migrate(
        &self,
        ctx: &Ctx,
        spec: &ReplaceSpec,
        backup: &mut Backup,
        deep: bool,
    ) -> Result<Vec<Finding>> {
        let mut actions = Vec::new();
        let (old_enc, new_enc) = (
            encodings::droid_bucket(&spec.old),
            encodings::droid_bucket(&spec.new),
        );
        for (o, n) in
            super::rename_encoded_children(&ctx.h(".factory/sessions"), &old_enc, &new_enc, backup)
        {
            actions.push(mk(
                self.name(),
                "dir_rename",
                &o,
                &format!("-> {}", n.display()),
            ));
        }
        // aggressive pass: session files embed the cwd in log-style lines,
        // so every extension is rewritten as raw text (not just identity
        // fields / config extensions)
        actions.extend(self.migrate_text_tree_aggressive(
            spec,
            backup,
            &[ctx.h(".factory")],
            deep,
        )?);
        Ok(actions)
    }
}

// ============================================================ crush

pub struct CrushAdapter;

impl CrushAdapter {
    fn roots(&self, ctx: &Ctx) -> Vec<PathBuf> {
        // Windows keeps the machine state under LOCALAPPDATA (crush
        // load.go GlobalConfigData), not the roaming APPDATA dir
        [ctx.dl("crush"), ctx.d("crush"), ctx.c("crush")]
            .into_iter()
            .filter(|p| p.exists())
            .collect()
    }

    /// <project>/.crush/crush.db — lives inside the moved project and
    /// carries absolute paths in files.path (edit version chains) and
    /// read_files.path
    fn project_db(&self, spec: &ReplaceSpec) -> Option<PathBuf> {
        let db = std::path::Path::new(&spec.old).join(".crush/crush.db");
        db.is_file().then_some(db)
    }
}

impl Adapter for CrushAdapter {
    fn name(&self) -> &'static str {
        "crush"
    }
    fn display(&self) -> &'static str {
        "Charm Crush"
    }
    fn note(&self) -> &'static str {
        "sessions live in <project>/.crush/crush.db (moves with the \
         project); global <data>/crush/projects.json holds {path, \
         data_dir} per project"
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["crush"]
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        self.roots(ctx)
    }

    fn root_kinds(&self) -> Vec<crate::ctx::RootKind> {
        // NOTE: existence filtering depends on ctx, which root_kinds
        // cannot see — callers must ZIP kinds against the FILTERED
        // state_paths and treat a missing pair as Home; the crush
        // kinds() helper exists for exact use, kept simple here
        vec![crate::ctx::RootKind::Data, crate::ctx::RootKind::Config]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = self.scan_tree(spec, &self.roots(ctx));
        if let Some(db) = self.project_db(spec) {
            if let Ok(con) = crate::sqlite::open_ro(&db) {
                let pat = spec.like_pattern();
                let n = super::sqlite_like_count(
                    &con,
                    "SELECT rowid FROM \"files\" WHERE \"path\" LIKE ?",
                    &pat,
                ) + super::sqlite_like_count(
                    &con,
                    "SELECT rowid FROM \"read_files\" WHERE \"path\" LIKE ?",
                    &pat,
                );
                if n > 0 {
                    out.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: format!("{}::files/read_files", db.display()),
                        detail: format!("{} rows", n),
                    });
                }
            }
        }
        out
    }

    fn migrate(
        &self,
        ctx: &Ctx,
        spec: &ReplaceSpec,
        backup: &mut Backup,
        deep: bool,
    ) -> Result<Vec<Finding>> {
        let mut actions = self.migrate_text_tree(spec, backup, &self.roots(ctx), deep)?;
        if let Some(db) = self.project_db(spec) {
            let n = crate::sqlite::open_ro(&db)
                .ok()
                .map(|con| {
                    super::sqlite_like_count(
                        &con,
                        "SELECT rowid FROM \"files\" WHERE \"path\" LIKE ?",
                        &spec.like_pattern(),
                    ) + super::sqlite_like_count(
                        &con,
                        "SELECT rowid FROM \"read_files\" WHERE \"path\" LIKE ?",
                        &spec.like_pattern(),
                    )
                })
                .unwrap_or(0);
            if n > 0 {
                backup.record_db(&db)?;
                if !backup.dry_run {
                    let con = crate::sqlite::open_rw(&db)?;
                    super::rewrite_pair(
                        &con,
                        &spec.like_patterns(),
                        spec,
                        "SELECT rowid,\"path\" FROM \"files\" \
                         WHERE \"path\" LIKE ?",
                        "UPDATE \"files\" SET \"path\"=? WHERE rowid=?",
                    )?;
                    super::rewrite_pair(
                        &con,
                        &spec.like_patterns(),
                        spec,
                        "SELECT rowid,\"path\" FROM \"read_files\" \
                         WHERE \"path\" LIKE ?",
                        "UPDATE \"read_files\" SET \"path\"=? WHERE rowid=?",
                    )?;
                }
                actions.push(mk(self.name(), "sqlite", &db, "files/read_files paths"));
            }
        }
        Ok(actions)
    }
}

// ============================================================ aider

pub struct AiderAdapter;

impl Adapter for AiderAdapter {
    fn name(&self) -> &'static str {
        "aider"
    }
    fn display(&self) -> &'static str {
        "Aider"
    }
    fn note(&self) -> &'static str {
        "~/.aider.conf.yml (abs paths in config); per-project .aider* \
         files live inside the project dir and move with it"
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["aider"]
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.h(".aider.conf.yml"), ctx.h(".aider")]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        self.scan_tree(spec, &[ctx.h(".aider.conf.yml")])
    }

    fn migrate(
        &self,
        ctx: &Ctx,
        spec: &ReplaceSpec,
        backup: &mut Backup,
        _deep: bool,
    ) -> Result<Vec<Finding>> {
        let mut actions = Vec::new();
        let conf = ctx.h(".aider.conf.yml");
        if conf.is_file() && rewriters::rewrite_text_file(&conf, spec, backup)? {
            actions.push(mk(self.name(), "file", &conf, "text"));
        }
        Ok(actions)
    }
}

// ============================================================ cc-connect

pub struct CcConnectAdapter;

impl CcConnectAdapter {
    fn dir_history(&self, ctx: &Ctx) -> PathBuf {
        ctx.h(".cc-connect/dir_history.json")
    }

    fn sessions(&self, ctx: &Ctx) -> PathBuf {
        ctx.h(".cc-connect/sessions")
    }

    fn root(&self, ctx: &Ctx) -> PathBuf {
        ctx.h(".cc-connect")
    }

    /// the whole data dir as one text root: sessions/, projects/
    /// (work_dir_override state), crons//timers/ job WorkDirs,
    /// config.toml work_dir/base_dir AND the legacy root-level
    /// hash-suffixed session files (renamed above, content rewritten
    /// here). run/ holds runtime sockets and short-lived state with no
    /// project paths.
    fn roots(&self, ctx: &Ctx) -> Vec<PathBuf> {
        let r = self.root(ctx);
        if r.exists() {
            vec![r]
        } else {
            Vec::new()
        }
    }
}

impl Adapter for CcConnectAdapter {
    fn name(&self) -> &'static str {
        "cc-connect"
    }
    fn display(&self) -> &'static str {
        "cc-connect"
    }
    fn note(&self) -> &'static str {
        "~/.cc-connect/dir_history.json (per-project dir MRU) + \
         sessions/<project>_<sha256(workDir)[:8]>.json file names"
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["cc-connect"]
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.h(".cc-connect")]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = self.scan_tree(spec, &[self.dir_history(ctx)]);
        out.extend(self.scan_tree(spec, &self.roots(ctx)));
        let old_h = encodings::sha256_8(&spec.old);
        // hash-suffixed session files live in sessions/ AND (legacy
        // layouts) directly in the data dir
        for dir in [self.sessions(ctx), self.root(ctx)] {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for e in entries.filter_map(|e| e.ok()) {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if name.contains(&format!("_{}.", old_h))
                        || name.contains(&format!("_{}.sessions.", old_h))
                    {
                        out.push(Finding {
                            agent: self.name().into(),
                            kind: "dir_rename".into(),
                            target: e.path().to_string_lossy().into_owned(),
                            detail: "hash-suffixed session file".into(),
                        });
                    }
                }
            }
        }
        out
    }

    fn migrate(
        &self,
        ctx: &Ctx,
        spec: &ReplaceSpec,
        backup: &mut Backup,
        deep: bool,
    ) -> Result<Vec<Finding>> {
        let mut actions = Vec::new();
        let dh = self.dir_history(ctx);
        if dh.is_file() && rewriters::rewrite_text_file(&dh, spec, backup)? {
            actions.push(mk(self.name(), "file", &dh, "text"));
        }
        let (old_h, new_h) = (
            encodings::sha256_8(&spec.old),
            encodings::sha256_8(&spec.new),
        );
        if old_h != new_h {
            for dir in [self.sessions(ctx), self.root(ctx)] {
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    let names: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect();
                    for name in names {
                        let tag = format!("_{}.", old_h);
                        if let Some(idx) = name.find(&tag) {
                            let old_f = dir.join(&name);
                            let new_name = format!(
                                "{}_{}{}",
                                &name[..idx],
                                new_h,
                                &name[idx + tag.len() - 1..]
                            );
                            let new_f = dir.join(new_name);
                            if old_f.is_file() && !new_f.exists() {
                                backup.record_rename(&old_f, &new_f);
                                if !backup.dry_run {
                                    std::fs::rename(&old_f, &new_f)?;
                                }
                                actions.push(mk(
                                    self.name(),
                                    "dir_rename",
                                    &old_f,
                                    &format!("-> {}", new_f.display()),
                                ));
                            }
                        }
                    }
                }
            }
        }
        actions.extend(self.migrate_text_tree(spec, backup, &self.roots(ctx), deep)?);
        Ok(actions)
    }
}

// ============================================================ generic

/// --extra-root: plain text/protobuf rewrite under any user-supplied root
pub struct GenericAdapter {
    pub root: PathBuf,
}

impl Adapter for GenericAdapter {
    fn name(&self) -> &'static str {
        "extra"
    }
    fn display(&self) -> &'static str {
        "extra root"
    }
    fn note(&self) -> &'static str {
        "user-supplied tree (--extra-root)"
    }

    fn state_paths(&self, _ctx: &Ctx) -> Vec<PathBuf> {
        vec![self.root.clone()]
    }

    fn installed(&self, _ctx: &Ctx) -> bool {
        self.root.exists()
    }

    fn scan(&self, _ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        self.scan_tree(spec, std::slice::from_ref(&self.root))
    }

    fn migrate(
        &self,
        _ctx: &Ctx,
        spec: &ReplaceSpec,
        backup: &mut Backup,
        _deep: bool,
    ) -> Result<Vec<Finding>> {
        let mut actions = self.migrate_text_tree_aggressive(
            spec,
            backup,
            std::slice::from_ref(&self.root),
            true,
        )?;
        actions.extend(self.migrate_pb_tree(spec, backup, std::slice::from_ref(&self.root))?);
        Ok(actions)
    }
}

// aggressive variant of `migrate_text_tree` for agent-owned state roots
// where any file may carry the path: every extension is rewritten as raw
// text (`aggressive = true`) and .pb files are rewritten inline; the
// standard SKIP_DIRS (caches, node_modules, telemetry, …) still apply
pub trait TextTreeExt: Adapter {
    fn migrate_text_tree_aggressive(
        &self,
        spec: &ReplaceSpec,
        backup: &mut Backup,
        roots: &[PathBuf],
        deep: bool,
    ) -> Result<Vec<Finding>> {
        let mut actions = Vec::new();
        for f in super::iter_files(roots, None) {
            if f.extension().map(|e| e == "pb").unwrap_or(false) {
                if super::rewrite_pb_file(&f, spec, backup)? {
                    actions.push(mk(self.name(), "protobuf", &f, "pb"));
                }
                continue;
            }
            if let Some(finding) =
                super::rewrite_file_by_ext(self.name(), &f, spec, backup, deep, true)?
            {
                actions.push(finding);
            }
        }
        Ok(actions)
    }
}

impl<T: Adapter + ?Sized> TextTreeExt for T {}
