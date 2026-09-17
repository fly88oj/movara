// SPDX-License-Identifier: MIT OR Apache-2.0
//! The `movara` command line.

use crate::adapters::misc::GenericAdapter;
use crate::adapters::{self, Adapter};
use crate::backup::{self, Backup};
use crate::ctx::Ctx;
use crate::spec::ReplaceSpec;
use anyhow::{bail, Context as _, Result};
use clap::{Args, Parser, Subcommand};
use rust_i18n::t;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "movara",
    version = crate::VERSION,
    about = "Migrate AI-agent session/config references when a project directory moves or is renamed"
)]
pub struct Cli {
    /// override the display language (en, zh-CN, ja, ko, es, fr, de, pt-BR)
    #[arg(long, global = true)]
    pub lang: Option<String>,

    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// list supported agents
    Agents {
        /// JSON output
        #[arg(long)]
        json: bool,
    },
    /// show what references a path
    Scan {
        #[command(flatten)]
        common: CommonArgs,
        /// old project path (absolute)
        #[arg(long = "from")]
        frm: PathBuf,
        /// only needed for rename-target previews in scan output
        #[arg(long = "to")]
        to: Option<PathBuf>,
    },
    /// rewrite old->new path references
    Migrate {
        #[command(flatten)]
        common: CommonArgs,
        #[arg(long = "from")]
        frm: PathBuf,
        #[arg(long = "to")]
        to: PathBuf,
        /// report only; change nothing
        #[arg(long)]
        dry_run: bool,
        /// also rewrite matches inside chat content/logs
        #[arg(long)]
        deep: bool,
        /// skip confirmation
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        backup_dir: Option<PathBuf>,
        /// move the project directory itself first
        #[arg(long)]
        move_project: bool,
    },
    /// move a project directory and rewrite all agent history in one step
    Mv {
        #[command(flatten)]
        common: CommonArgs,
        /// project directory to move
        src: PathBuf,
        /// destination (like mv: existing dir = move into it)
        dst: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        deep: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        backup_dir: Option<PathBuf>,
    },
    /// revert a migration
    Undo {
        #[arg(long = "id")]
        id: String,
        #[arg(long)]
        backup_dir: Option<PathBuf>,
    },
    /// list migrations
    Backups {
        #[arg(long)]
        backup_dir: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Args)]
pub struct CommonArgs {
    /// comma list of agents (default: all installed)
    #[arg(long)]
    pub agents: Option<String>,
    /// additional tree to rewrite (repeatable)
    #[arg(long = "extra-root")]
    pub extra_root: Vec<PathBuf>,
    /// JSON output
    #[arg(long)]
    pub json: bool,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    crate::i18n::set_locale_from_args(cli.lang.as_deref());
    let ctx = Ctx::from_env();
    match &cli.cmd {
        Cmd::Agents { json } => cmd_agents(&ctx, *json),
        Cmd::Scan { common, frm, to } => cmd_scan(&ctx, common, frm, to.as_deref()),
        Cmd::Migrate {
            common,
            frm,
            to,
            dry_run,
            deep,
            yes,
            backup_dir,
            move_project,
        } => cmd_migrate(
            &ctx,
            common,
            frm,
            to,
            *dry_run,
            *deep,
            *yes,
            backup_dir.clone(),
            *move_project,
        ),
        Cmd::Mv {
            common,
            src,
            dst,
            dry_run,
            deep,
            yes,
            backup_dir,
        } => cmd_mv(
            &ctx,
            common,
            src,
            dst,
            *dry_run,
            *deep,
            *yes,
            backup_dir.clone(),
        ),
        Cmd::Undo { id, backup_dir } => {
            let dir = backup_dir
                .clone()
                .unwrap_or_else(|| ctx.default_backup_dir());
            if backup::undo(&dir, id)? {
                println!("{}", t!("undo.done", id = id.as_str()));
                Ok(())
            } else {
                println!("{}", t!("undo.warnings", id = id.as_str()));
                std::process::exit(1);
            }
        }
        Cmd::Backups { backup_dir, json } => cmd_backups(
            backup_dir
                .clone()
                .unwrap_or_else(|| ctx.default_backup_dir()),
            *json,
        ),
    }
}

fn adapters_for(common: &CommonArgs) -> Result<Vec<Box<dyn Adapter>>> {
    let names: Option<Vec<String>> = common
        .agents
        .as_ref()
        .map(|a| a.split(',').map(|s| s.trim().to_string()).collect());
    let mut list = adapters::get_adapters(names.as_deref())?;
    for root in &common.extra_root {
        list.push(Box::new(GenericAdapter {
            root: std::fs::canonicalize(root).unwrap_or_else(|_| root.clone()),
        }));
    }
    Ok(list)
}

fn cmd_agents(ctx: &Ctx, json: bool) -> Result<()> {
    let all = adapters::all();
    if json {
        let rows: Vec<serde_json::Value> = all
            .iter()
            .map(|a| {
                serde_json::json!({
                    "name": a.name(),
                    "display": a.display(),
                    "installed": a.installed(ctx),
                    "note": a.note(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    println!(
        "{:<12} {:<10} {}",
        t!("agents.col_agent"),
        t!("agents.col_installed"),
        t!("agents.col_desc")
    );
    for a in &all {
        println!(
            "{:<12} {:<10} {}: {}",
            a.name(),
            if a.installed(ctx) {
                t!("agents.yes").to_string()
            } else {
                "-".to_string()
            },
            a.display(),
            a.note()
        );
    }
    Ok(())
}

fn cmd_scan(ctx: &Ctx, common: &CommonArgs, frm: &Path, to: Option<&Path>) -> Result<()> {
    let to_str = to
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| frm.to_string_lossy().into_owned());
    let spec = ReplaceSpec::new(&frm.to_string_lossy(), &to_str)?;
    let adapters = adapters_for(common)?;
    let mut findings = Vec::new();
    for a in &adapters {
        if a.installed(ctx) {
            findings.extend(a.scan(ctx, &spec));
        }
    }
    if common.json {
        println!("{}", serde_json::to_string_pretty(&findings)?);
        return Ok(());
    }
    let mut current = String::new();
    for f in &findings {
        if f.agent != current {
            current = f.agent.clone();
            println!("\n[{}]", current);
        }
        println!("  {:<10} {}  {}", f.kind, f.target, f.detail);
    }
    let installed = adapters.iter().filter(|a| a.installed(ctx)).count();
    println!(
        "\n{}",
        t!("scan.summary", agents = installed, refs = findings.len())
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_migrate(
    ctx: &Ctx,
    common: &CommonArgs,
    frm: &Path,
    to: &Path,
    dry_run: bool,
    deep: bool,
    yes: bool,
    backup_dir: Option<PathBuf>,
    move_project: bool,
) -> Result<()> {
    let spec = ReplaceSpec::new(&frm.to_string_lossy(), &to.to_string_lossy())?;
    if spec.old == spec.new {
        anyhow::bail!("{}", t!("migrate.err_same", path = spec.old.as_str()));
    }
    let adapters = adapters_for(common)?;
    if !yes && !dry_run {
        print!(
            "{}",
            t!(
                "migrate.confirm",
                from = spec.old.as_str(),
                to = spec.new.as_str()
            )
        );
        let _ = std::io::stdout().flush();
        let mut ans = String::new();
        std::io::stdin().read_line(&mut ans)?;
        if !ans.trim().eq_ignore_ascii_case("y") && !ans.trim().eq_ignore_ascii_case("yes") {
            println!("{}", t!("common.aborted"));
            return Ok(());
        }
    }
    let backup_dir = backup_dir.unwrap_or_else(|| ctx.default_backup_dir());
    let mut backup = Backup::new(
        &backup_dir,
        &spec,
        adapters.iter().map(|a| a.name().to_string()).collect(),
        dry_run,
    );
    if move_project {
        let (old, new) = (Path::new(&spec.old), Path::new(&spec.new));
        if old.is_dir() && !new.exists() {
            backup.manifest.moved_project = Some((spec.old.clone(), spec.new.clone()));
            if !dry_run {
                if let Some(parent) = new.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                move_dir(old, new)?;
                if !common.json {
                    println!(
                        "{}",
                        t!(
                            "migrate.moved",
                            from = spec.old.as_str(),
                            to = spec.new.as_str()
                        )
                    );
                }
            }
        } else if new.exists() && !common.json {
            println!("{}", t!("migrate.move_skipped", to = spec.new.as_str()));
        }
    }
    let (total, n_agents, report) =
        run_migrations(ctx, &adapters, &spec, &mut backup, deep, common.json);
    backup.save().context("saving backup manifest")?;
    if common.json {
        println!(
            "{}",
            serde_json::json!({
                "backup_id": if dry_run { None } else { Some(backup.manifest.id.clone()) },
                "changes": total,
                "report": report,
            })
        );
    } else {
        let mode = if dry_run {
            t!("migrate.would_change")
        } else {
            t!("migrate.changed")
        };
        println!(
            "\n{}",
            t!(
                "migrate.summary",
                count = total,
                mode = mode.as_ref(),
                agents = n_agents
            )
        );
        if !dry_run {
            println!(
                "{}",
                t!("migrate.undo_hint", id = backup.manifest.id.as_str())
            );
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_mv(
    ctx: &Ctx,
    common: &CommonArgs,
    src: &Path,
    dst: &Path,
    dry_run: bool,
    deep: bool,
    yes: bool,
    backup_dir: Option<PathBuf>,
) -> Result<()> {
    let (src_abs, new_abs) = mv_resolve(src, dst)?;
    let spec = ReplaceSpec::new(&src_abs.to_string_lossy(), &new_abs.to_string_lossy())?;
    let adapters = adapters_for(common)?;
    // preflight
    let mut findings = Vec::new();
    for a in &adapters {
        if a.installed(ctx) {
            findings.extend(a.scan(ctx, &spec));
        }
    }
    let agents_hit: std::collections::BTreeSet<&str> =
        findings.iter().map(|f| f.agent.as_str()).collect();
    if !common.json {
        println!(
            "{}  {}\n  ->  {}",
            t!("mv.move"),
            src_abs.display(),
            new_abs.display()
        );
        println!(
            "{}: {}{}",
            t!("mv.agents"),
            if agents_hit.is_empty() {
                t!("mv.none").to_string()
            } else {
                agents_hit.len().to_string()
            },
            if findings.is_empty() {
                String::new()
            } else {
                t!("mv.refs_suffix", count = findings.len()).to_string()
            }
        );
    }
    if dry_run {
        if !common.json {
            for f in &findings {
                println!(
                    "  {} [{}] {}: {}",
                    t!("mv.would_rewrite"),
                    f.agent,
                    f.kind,
                    f.target
                );
            }
            println!("{}", t!("mv.dry_run"));
            return Ok(());
        }
        println!(
            "{}",
            serde_json::json!({
                "dry_run": true,
                "would_change": findings.len(),
                "findings": findings,
            })
        );
        return Ok(());
    }
    if !yes {
        print!("{}", t!("mv.proceed"));
        let _ = std::io::stdout().flush();
        let mut ans = String::new();
        std::io::stdin().read_line(&mut ans)?;
        if !ans.trim().eq_ignore_ascii_case("y") && !ans.trim().eq_ignore_ascii_case("yes") {
            if !common.json {
                println!("{}", t!("common.aborted"));
            }
            return Ok(());
        }
    }
    let backup_dir = backup_dir.unwrap_or_else(|| ctx.default_backup_dir());
    let mut backup = Backup::new(
        &backup_dir,
        &spec,
        adapters.iter().map(|a| a.name().to_string()).collect(),
        false,
    );
    backup.manifest.moved_project = Some((spec.old.clone(), spec.new.clone()));
    move_dir(Path::new(&spec.old), Path::new(&spec.new)).context("moving the project directory")?;
    if !common.json {
        println!("{} {} -> {}", t!("mv.moved"), spec.old, spec.new);
    }
    let (total, n_agents, report) =
        run_migrations(ctx, &adapters, &spec, &mut backup, deep, common.json);
    backup.save().context("saving backup manifest")?;
    if common.json {
        println!(
            "{}",
            serde_json::json!({
                "backup_id": backup.manifest.id,
                "changes": total,
                "report": report,
            })
        );
    } else {
        println!("\n{}", t!("mv.summary", count = total, agents = n_agents));
        println!("{}: movara undo --id {}", t!("mv.undo"), backup.manifest.id);
    }
    Ok(())
}

fn run_migrations(
    ctx: &Ctx,
    adapters: &[Box<dyn Adapter>],
    spec: &ReplaceSpec,
    backup: &mut Backup,
    deep: bool,
    quiet: bool,
) -> (usize, usize, serde_json::Value) {
    let mut total = 0;
    let mut n_agents = 0;
    let mut report = serde_json::Map::new();
    for a in adapters {
        if !a.installed(ctx) {
            continue;
        }
        let actions = match a.migrate(ctx, spec, backup, deep) {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "{}",
                    t!(
                        "common.agent_error",
                        agent = a.name(),
                        error = e.to_string().as_str()
                    )
                );
                vec![adapters::Finding {
                    agent: a.name().to_string(),
                    kind: "error".into(),
                    target: format!("{:?}", e),
                    detail: String::new(),
                }]
            }
        };
        if !actions.is_empty() {
            total += actions.len();
            n_agents += 1;
            let entries: Vec<serde_json::Value> = actions
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "kind": f.kind, "target": f.target,
                        "detail": f.detail,
                    })
                })
                .collect();
            report.insert(a.name().to_string(), entries.into());
            if !quiet {
                println!(
                    "{}",
                    t!(
                        "common.agent_changes",
                        agent = a.name(),
                        count = actions.len()
                    )
                );
            }
        }
    }
    (total, n_agents, serde_json::Value::Object(report))
}

/// mv-like semantics: dst that is an existing directory = move into it
pub fn mv_resolve(src: &Path, dst: &Path) -> Result<(PathBuf, PathBuf)> {
    let src_abs = crate::spec::absolutish(src);
    let dst_abs = crate::spec::absolutish(dst);
    if !src_abs.is_dir() {
        bail!(t!(
            "mv.err_not_dir",
            src = src.display().to_string().as_str()
        ));
    }
    let new_abs = if dst_abs.is_dir() {
        let base = src_abs
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_default();
        dst_abs.join(base)
    } else {
        dst_abs
    };
    // checked before the exists() test: src == dst always implies the
    // target exists (src was just validated as a directory), so the more
    // specific error must come first
    if src_abs == new_abs {
        bail!(t!("mv.err_same"));
    }
    if new_abs.exists() {
        bail!(t!(
            "mv.err_target_exists",
            target = new_abs.display().to_string().as_str()
        ));
    }
    if let Some(parent) = new_abs.parent() {
        if !parent.is_dir() {
            bail!(t!(
                "mv.err_no_parent",
                parent = parent.display().to_string().as_str()
            ));
        }
    }
    Ok((src_abs, new_abs))
}

/// rename with cross-filesystem fallback (copy + remove)
pub fn move_dir(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(e) if matches!(e.raw_os_error(), Some(18) | Some(17)) => {
            // 18 = EXDEV (POSIX), 17 = ERROR_NOT_SAME_DEVICE (Windows)
            backup::copy_entry(src, dst)?;
            std::fs::remove_dir_all(src)?;
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

fn cmd_backups(dir: PathBuf, json: bool) -> Result<()> {
    let rows = backup::list_backups(&dir)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    if rows.is_empty() {
        println!("{}", t!("backups.none"));
        return Ok(());
    }
    for m in &rows {
        println!(
            "{}",
            t!(
                "backups.row",
                id = m.id.as_str(),
                from = m.from.as_str(),
                to = m.to.as_str(),
                agents = m.agents.join(",").as_str(),
                files = m.files.len(),
                dbs = m.dbs.len(),
                renames = m.renames.len()
            )
        );
    }
    Ok(())
}
