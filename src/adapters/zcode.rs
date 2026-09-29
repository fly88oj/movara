// SPDX-License-Identifier: MIT OR Apache-2.0
//! ZCode (Z.ai coding agent harness).
//!
//! Layout verified against the open-source tree (zai-org/ZCode v3.14)
//! and live 3.10+ state dirs:
//! - ~/.zcode/cli/db/db.sqlite
//!   session.directory/path, session.project_id (proj_ + slugified
//!   path [:80]; the runtime rederives it from the workspace root and
//!   listSessions filters by it, so a stale id orphans the sessions),
//!   permission/input_history.project_id, local_setting.scope_id
//!   (project scope) and its ruleset values, workflow_run.cwd/
//!   script_path, workflow_definition.script_path, dwf_run.cwd
//! - ~/.zcode/cli/agents/sess_<id>/agent_<id>/metadata.json  workspace
//!   path, exec/<sess>, artifacts/<sess>, rollout/*.jsonl, config.json
//! - ~/.zcode/cli/memories/projects/<slug48>-<sha256(cwd)[:16]>/
//! - ~/.zcode/v2 (desktop side): bot-state.v{2,3}.json
//!   bots[].workspacePath/workspaceId (raw paths), setting.json
//!   recentProjects + lastWorkspaceSession, checkpoints|sessions/
//!   <sha256(cwd)[:12]>/, tasks-index.sqlite workspace_path columns

use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::encodings;
use crate::spec::ReplaceSpec;
use crate::sqlite;
use anyhow::Result;
use std::path::PathBuf;

pub struct ZcodeAdapter;

impl ZcodeAdapter {
    fn db(&self, ctx: &Ctx) -> PathBuf {
        ctx.h(".zcode/cli/db/db.sqlite")
    }

    fn tasks_db(&self, ctx: &Ctx) -> PathBuf {
        ctx.h(".zcode/v2/tasks-index.sqlite")
    }

    fn roots(&self, ctx: &Ctx) -> Vec<PathBuf> {
        [
            ".zcode/cli/agents",
            ".zcode/cli/exec",
            ".zcode/cli/artifacts",
            ".zcode/cli/rollout",
            ".zcode/cli/config.json",
            ".zcode/v2/checkpoints",
            ".zcode/v2/sessions",
            ".zcode/v2/bot-state.v2.json",
            ".zcode/v2/bot-state.v3.json",
            ".zcode/v2/setting.json",
            ".zcode/v2/config.json",
        ]
        .iter()
        .map(|rel| ctx.h(rel))
        .filter(|p| p.exists())
        .collect()
    }

    fn memories(&self, ctx: &Ctx) -> PathBuf {
        ctx.h(".zcode/cli/memories/projects")
    }

    /// path-keyed directories on the desktop side (v2/checkpoints and
    /// v2/sessions share the sha256(cwd)[:12] naming)
    fn hash_dirs(&self, ctx: &Ctx) -> Vec<PathBuf> {
        [".zcode/v2/checkpoints", ".zcode/v2/sessions"]
            .iter()
            .map(|rel| ctx.h(rel))
            .filter(|p| p.is_dir())
            .collect()
    }
}

impl Adapter for ZcodeAdapter {
    fn name(&self) -> &'static str {
        "zcode"
    }
    fn display(&self) -> &'static str {
        "ZCode"
    }
    fn note(&self) -> &'static str {
        "db.sqlite session.directory/path + project_id identity, \
         workflow_run/dwf_run cwd, memories/projects/<slug>-<sha256[:16]>, \
         desktop v2 bot-state/setting/checkpoints/tasks-index"
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["zcode"]
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.h(".zcode")]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        let old_pid = encodings::zcode_project_id(&spec.old);
        let new_pid = encodings::zcode_project_id(&spec.new);
        let db = self.db(ctx);
        if db.is_file() {
            if let Ok(con) = sqlite::open_ro(&db) {
                let pat = spec.like_pattern();
                let n = super::sqlite_like_count(
                    &con,
                    "SELECT \"id\" FROM \"session\" \
                     WHERE \"directory\" LIKE ?",
                    &pat,
                ) + super::sqlite_like_count(
                    &con,
                    "SELECT \"id\" FROM \"session\" \
                     WHERE \"path\" LIKE ?",
                    &pat,
                );
                if n > 0 {
                    out.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: format!("{}::session", db.display()),
                        detail: format!("{} rows", n),
                    });
                }
                let n = super::sqlite_like_count(
                    &con,
                    "SELECT \"id\" FROM \"workflow_run\" \
                     WHERE \"cwd\" LIKE ?",
                    &pat,
                );
                if n > 0 {
                    out.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: format!("{}::workflow_run", db.display()),
                        detail: format!("{} rows", n),
                    });
                }
                // project-identity tokens: exact match, not LIKE
                let mut keys = 0usize;
                for (table, col) in [
                    ("session", "project_id"),
                    ("permission", "project_id"),
                    ("input_history", "project_id"),
                    ("local_setting", "scope_id"),
                ] {
                    keys += super::sqlite_like_count(
                        &con,
                        &format!("SELECT rowid FROM \"{}\" WHERE \"{}\" = ?", table, col),
                        &old_pid,
                    );
                }
                if keys > 0 {
                    out.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: format!("{}::project-identity", db.display()),
                        detail: format!("{} rows {} -> {}", keys, old_pid, new_pid),
                    });
                }
                let extra = super::sqlite_like_count(
                    &con,
                    "SELECT rowid FROM \"local_setting\" \
                     WHERE \"value\" LIKE ?",
                    &pat,
                ) + super::sqlite_like_count(
                    &con,
                    "SELECT \"id\" FROM \"dwf_run\" WHERE \"cwd\" LIKE ?",
                    &pat,
                ) + super::sqlite_like_count(
                    &con,
                    "SELECT \"id\" FROM \"workflow_run\" \
                     WHERE \"script_path\" LIKE ?",
                    &pat,
                ) + super::sqlite_like_count(
                    &con,
                    "SELECT \"id\" FROM \"workflow_definition\" \
                     WHERE \"script_path\" LIKE ?",
                    &pat,
                );
                if extra > 0 {
                    out.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: format!("{}::rulesets-and-paths", db.display()),
                        detail: format!("{} rows", extra),
                    });
                }
            }
        }
        let mem = self.memories(ctx);
        let old_key = encodings::zcode_memory_key(&spec.old);
        let old_d = mem.join(&old_key);
        if old_d.is_dir() {
            out.push(Finding {
                agent: self.name().into(),
                kind: "dir_rename".into(),
                target: old_d.to_string_lossy().into_owned(),
                detail: "-> ".to_string() + &encodings::zcode_memory_key(&spec.new),
            });
        }
        let old_h12 = encodings::zcode_workspace_hash12(&spec.old);
        for parent in self.hash_dirs(ctx) {
            let old_d = parent.join(&old_h12);
            if old_d.is_dir() {
                out.push(Finding {
                    agent: self.name().into(),
                    kind: "dir_rename".into(),
                    target: old_d.to_string_lossy().into_owned(),
                    detail: "-> ".to_string()
                        + &parent
                            .join(encodings::zcode_workspace_hash12(&spec.new))
                            .file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                });
            }
        }
        let tdb = self.tasks_db(ctx);
        if tdb.is_file() {
            if let Ok(con) = sqlite::open_ro(&tdb) {
                let pat = spec.like_pattern();
                let mut n = 0usize;
                for col in ["workspace_path", "workspace_key"] {
                    n += super::sqlite_like_count(
                        &con,
                        &format!("SELECT task_id FROM \"tasks\" WHERE \"{}\" LIKE ?", col),
                        &pat,
                    );
                }
                if n > 0 {
                    out.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: format!("{}::tasks", tdb.display()),
                        detail: format!("{} rows", n),
                    });
                }
            }
        }
        out.extend(self.scan_tree(spec, &self.roots(ctx)));
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
        let db = self.db(ctx);
        if db.is_file() {
            backup.record_db(&db)?;
            let mut updated = 0usize;
            if !backup.dry_run {
                let con = sqlite::open_rw(&db)?;
                updated += super::rewrite_pair(
                    &con,
                    &spec.like_patterns(),
                    spec,
                    "SELECT \"id\",\"directory\" FROM \"session\" \
                     WHERE \"directory\" LIKE ?",
                    "UPDATE \"session\" SET \"directory\"=? \
                     WHERE \"id\"=?",
                )?;
                updated += super::rewrite_pair(
                    &con,
                    &spec.like_patterns(),
                    spec,
                    "SELECT \"id\",\"path\" FROM \"session\" \
                     WHERE \"path\" LIKE ?",
                    "UPDATE \"session\" SET \"path\"=? WHERE \"id\"=?",
                )?;
                updated += super::rewrite_pair(
                    &con,
                    &spec.like_patterns(),
                    spec,
                    "SELECT \"id\",\"cwd\" FROM \"workflow_run\" \
                     WHERE \"cwd\" LIKE ?",
                    "UPDATE \"workflow_run\" SET \"cwd\"=? \
                     WHERE \"id\"=?",
                )?;
                // exact-match token swaps (optional tables may not exist
                // in older databases)
                let old_pid = encodings::zcode_project_id(&spec.old);
                let new_pid = encodings::zcode_project_id(&spec.new);
                if old_pid != new_pid {
                    for (table, col) in [
                        ("session", "project_id"),
                        ("permission", "project_id"),
                        ("input_history", "project_id"),
                        ("local_setting", "scope_id"),
                    ] {
                        let _ = con
                            .execute(
                                &format!(
                                    "UPDATE \"{}\" SET \"{}\"=? WHERE \"{}\"=?",
                                    table, col, col
                                ),
                                rusqlite::params![new_pid, old_pid],
                            )
                            .map(|n| updated += n);
                    }
                }
                for (select, update) in [
                    (
                        "SELECT rowid,\"value\" FROM \"local_setting\" \
                         WHERE \"value\" LIKE ?",
                        "UPDATE \"local_setting\" SET \"value\"=? \
                         WHERE rowid=?",
                    ),
                    (
                        "SELECT \"id\",\"cwd\" FROM \"dwf_run\" \
                         WHERE \"cwd\" LIKE ?",
                        "UPDATE \"dwf_run\" SET \"cwd\"=? WHERE \"id\"=?",
                    ),
                    (
                        "SELECT \"id\",\"script_path\" FROM \"workflow_run\" \
                         WHERE \"script_path\" LIKE ?",
                        "UPDATE \"workflow_run\" SET \"script_path\"=? \
                         WHERE \"id\"=?",
                    ),
                    (
                        "SELECT \"id\",\"script_path\" \
                         FROM \"workflow_definition\" \
                         WHERE \"script_path\" LIKE ?",
                        "UPDATE \"workflow_definition\" SET \"script_path\"=? \
                         WHERE \"id\"=?",
                    ),
                ] {
                    let _ = super::rewrite_pair(&con, &spec.like_patterns(), spec, select, update)
                        .map(|n| updated += n);
                }
            }
            if updated > 0 {
                actions.push(mk(
                    self.name(),
                    "sqlite",
                    &db,
                    &format!("session/workflow_run rows updated: {}", updated),
                ));
            }
        }
        let mem = self.memories(ctx);
        let old_key = encodings::zcode_memory_key(&spec.old);
        let new_key = encodings::zcode_memory_key(&spec.new);
        let old_d = mem.join(&old_key);
        let new_d = mem.join(&new_key);
        if super::rename_dir(&old_d, &new_d, backup) {
            actions.push(mk(
                self.name(),
                "dir_rename",
                &old_d,
                &format!("-> {}", new_d.display()),
            ));
        }
        let old_h12 = encodings::zcode_workspace_hash12(&spec.old);
        let new_h12 = encodings::zcode_workspace_hash12(&spec.new);
        if old_h12 != new_h12 {
            for parent in self.hash_dirs(ctx) {
                let old_d = parent.join(&old_h12);
                let new_d = parent.join(&new_h12);
                if super::rename_dir(&old_d, &new_d, backup) {
                    actions.push(mk(
                        self.name(),
                        "dir_rename",
                        &old_d,
                        &format!("-> {}", new_d.display()),
                    ));
                }
            }
        }
        let tdb = self.tasks_db(ctx);
        if tdb.is_file() {
            backup.record_db(&tdb)?;
            let mut updated = 0usize;
            if !backup.dry_run {
                let con = sqlite::open_rw(&tdb)?;
                for (table, pk, col) in [
                    ("tasks", "task_id", "workspace_path"),
                    ("tasks", "task_id", "workspace_key"),
                    ("automations", "automation_id", "workspace_path"),
                    ("automations", "automation_id", "workspace_key"),
                    ("task_group_members", "task_id", "workspace_path"),
                    ("task_group_members", "task_id", "workspace_key"),
                    ("off_peak_tasks", "off_peak_task_id", "workspace_path"),
                    ("off_peak_tasks", "off_peak_task_id", "workspace_key"),
                    ("automation_runs", "run_id", "workspace_key"),
                    (
                        "task_group_workspace_bootstraps",
                        "workspace_key",
                        "workspace_key",
                    ),
                ] {
                    // the bootstrap table is keyed BY the column itself:
                    // fall back to rowid addressing
                    let (select, update) = if pk == col {
                        (
                            format!(
                                "SELECT rowid,\"{}\" FROM \"{}\" \
                                 WHERE \"{}\" LIKE ?",
                                col, table, col
                            ),
                            format!("UPDATE \"{}\" SET \"{}\"=? WHERE rowid=?", table, col),
                        )
                    } else {
                        (
                            format!(
                                "SELECT \"{}\",\"{}\" FROM \"{}\" \
                                 WHERE \"{}\" LIKE ?",
                                pk, col, table, col
                            ),
                            format!("UPDATE \"{}\" SET \"{}\"=? WHERE \"{}\"=?", table, col, pk),
                        )
                    };
                    let _ =
                        super::rewrite_pair(&con, &spec.like_patterns(), spec, &select, &update)
                            .map(|n| updated += n);
                }
            }
            if updated > 0 {
                actions.push(mk(
                    self.name(),
                    "sqlite",
                    &tdb,
                    &format!("tasks rows updated: {}", updated),
                ));
            }
        }
        actions.extend(self.migrate_text_tree(spec, backup, &self.roots(ctx), deep)?);
        Ok(actions)
    }
}
