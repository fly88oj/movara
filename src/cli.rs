// SPDX-License-Identifier: MIT OR Apache-2.0
//! The `movara` command line.

use crate::adapters::misc::GenericAdapter;
use crate::adapters::{self, Adapter};
use crate::backup::{self, Backup};
use crate::ctx::Ctx;
use crate::spec::ReplaceSpec;
use crate::sync;
use anyhow::{bail, Context as _, Result};
use clap::{Args, Parser, Subcommand};
use rust_i18n::t;
use std::collections::BTreeMap;
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
    /// export agent state to a portable archive
    Export {
        /// archive path (default movara-export-<timestamp>.tar.gz)
        #[arg(long)]
        out: Option<PathBuf>,
        /// only state referencing this project path (repeatable)
        #[arg(long = "path")]
        paths: Vec<PathBuf>,
        /// comma list of agents (default: all installed)
        #[arg(long)]
        agents: Option<String>,
        /// JSON output
        #[arg(long)]
        json: bool,
    },
    /// import an archive, optionally rebasing paths
    Import {
        archive: PathBuf,
        /// path mapping OLD:NEW (repeatable)
        #[arg(long = "rebase")]
        rebase: Vec<String>,
        /// comma list of agents to import (default: all in the archive)
        #[arg(long)]
        agents: Option<String>,
        /// conflict policy for existing local state (skip | replace)
        #[arg(long = "on-conflict", default_value = "skip")]
        on_conflict: String,
        /// proceed even when archive paths resolve to nothing locally
        #[arg(long = "allow-missing-path", default_value_t = false)]
        allow_missing_path: bool,
        /// report only
        #[arg(long = "dry-run")]
        dry_run: bool,
        /// skip confirmation
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        backup_dir: Option<PathBuf>,
        /// JSON output
        #[arg(long)]
        json: bool,
    },
    /// move a project and its agent state to another host over ssh
    Move {
        /// project directory to move
        src: PathBuf,
        /// destination as [user@]host:<path> (no IPv6 literals)
        dest: String,
        /// carry only agent state + project memory, not the code
        #[arg(long = "state-only")]
        state_only: bool,
        /// remove the moved state set on this host after verified success
        #[arg(long)]
        cleanup: bool,
        /// report only
        #[arg(long = "dry-run")]
        dry_run: bool,
        /// skip confirmation
        #[arg(long)]
        yes: bool,
        /// comma list of agents (default: all installed)
        #[arg(long)]
        agents: Option<String>,
    },
    /// [move target side] receive a streamed archive from stdin
    Receive {
        /// destination directory for the project on this host
        #[arg(long = "dst")]
        dst: PathBuf,
        /// preflight only: check the destination, then exit
        #[arg(long = "plan-only")]
        plan_only: bool,
        /// non-interactive (required when streaming)
        #[arg(long)]
        yes: bool,
        /// JSON output
        #[arg(long)]
        json: bool,
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
    /// sync a registered pair's project state with the other host
    Sync {
        /// pair name to run now (see `movara sync pair list`)
        name: Option<String>,
        /// liveness freshness window in seconds
        #[arg(long, default_value_t = 180)]
        freshness: u64,
        /// report the plan only; change nothing
        #[arg(long = "dry-run")]
        dry_run: bool,
        #[arg(long)]
        backup_dir: Option<PathBuf>,
        /// JSON output
        #[arg(long)]
        json: bool,
        #[command(subcommand)]
        cmd: Option<SyncSub>,
    },
    /// [internal] sync protocol steps, driven over ssh by `movara sync`
    #[command(hide = true)]
    SyncAgent {
        #[command(subcommand)]
        cmd: AgentCmd,
    },
}

#[derive(Subcommand, Clone)]
pub enum SyncSub {
    /// manage sync pairs
    Pair {
        #[command(subcommand)]
        cmd: PairCmd,
    },
    /// show a pair's sync status (advisory; takes no lease)
    Status {
        name: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Clone)]
pub enum PairCmd {
    /// register a pair
    Add {
        name: String,
        /// absolute project path on this host
        #[arg(long)]
        local: PathBuf,
        /// remote ssh destination [user@]host
        #[arg(long)]
        host: String,
        /// absolute project path on the other host
        #[arg(long)]
        remote: String,
        /// also sync the project tree (roadmap; state sync only for now)
        #[arg(long = "with-files")]
        with_files: bool,
        /// comma list of agents (default: all installed)
        #[arg(long)]
        agents: Option<String>,
    },
    /// list registered pairs
    List {
        #[arg(long)]
        json: bool,
    },
    /// remove a pair registration (state on both hosts is untouched)
    Remove { name: String },
}

#[derive(Subcommand, Clone)]
pub enum AgentCmd {
    /// liveness probe; JSON ProbeReport on stdout
    Probe {
        #[arg(long)]
        project: String,
        #[arg(long, default_value_t = 180)]
        freshness: u64,
        #[arg(long)]
        agents: Option<String>,
    },
    /// member inventory; JSON map on stdout
    Inventory {
        #[arg(long)]
        project: String,
        /// the OTHER host's project path (hash canonicalization)
        #[arg(long)]
        other: String,
        #[arg(long)]
        pair: String,
        #[arg(long)]
        agents: Option<String>,
    },
    /// read members' bytes; stdin: JSON array of members, stdout: {"member": base64}
    Pack {
        #[arg(long)]
        project: String,
        #[arg(long)]
        agents: Option<String>,
    },
    /// apply one half of a plan; stdin: {"plan": [...], "staging": {"member": base64}},
    /// stdout: JSON ApplyReport
    Apply {
        #[arg(long)]
        project: String,
        #[arg(long)]
        other: String,
        #[arg(long)]
        pair: String,
        /// which endpoint this host is (a | b)
        #[arg(long)]
        side: String,
        #[arg(long)]
        run: String,
        #[arg(long)]
        agents: Option<String>,
    },
    /// persist the authoritative ledger; stdin: JSON LedgerState
    Commit {
        #[arg(long)]
        pair: String,
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
        Cmd::Export {
            out,
            paths,
            agents,
            json,
        } => cmd_export(&ctx, out.as_deref(), paths, agents.as_deref(), *json),
        Cmd::Import {
            archive,
            rebase,
            agents,
            on_conflict,
            allow_missing_path,
            dry_run,
            yes,
            backup_dir,
            json,
        } => cmd_import(
            &ctx,
            archive,
            rebase,
            agents.as_deref(),
            on_conflict,
            *allow_missing_path,
            *dry_run,
            *yes,
            backup_dir.clone(),
            *json,
        ),
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
        Cmd::Move {
            src,
            dest,
            state_only,
            cleanup,
            dry_run,
            yes,
            agents,
        } => cmd_move(
            &ctx,
            src,
            dest,
            *state_only,
            *cleanup,
            *dry_run,
            *yes,
            agents.as_deref(),
        ),
        Cmd::Receive {
            dst,
            plan_only,
            yes,
            json,
        } => cmd_receive(&ctx, dst, *plan_only, *yes, *json),
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
        Cmd::Sync {
            name,
            freshness,
            dry_run,
            backup_dir,
            json,
            cmd,
        } => match cmd {
            Some(SyncSub::Pair { cmd }) => cmd_sync_pair(&ctx, (*cmd).clone()),
            Some(SyncSub::Status { name, json }) => cmd_sync_status(&ctx, name, *json),
            None => match name {
                Some(n) => cmd_sync(&ctx, n, *freshness, *dry_run, backup_dir.clone(), *json),
                None => bail!("{}", t!("sync.err_name")),
            },
        },
        Cmd::SyncAgent { cmd } => cmd_sync_agent(&ctx, (*cmd).clone()),
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

fn cmd_export(
    ctx: &Ctx,
    out: Option<&Path>,
    paths: &[PathBuf],
    agents: Option<&str>,
    json: bool,
) -> Result<()> {
    let out = out.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        PathBuf::from(format!(
            "movara-export-{}.tar.gz",
            chrono::Local::now().format("%Y%m%d-%H%M%S-%6f")
        ))
    });
    let names: Option<Vec<String>> =
        agents.map(|a| a.split(',').map(|s| s.trim().to_string()).collect());
    let sel: Vec<String> = paths
        .iter()
        .map(|p| crate::ctx::path_str(&crate::spec::absolutish(p)))
        .collect();
    let list = adapters::get_adapters(names.as_deref())?;
    let opts = crate::archive::ExportOpts {
        out,
        filtered: names.is_some() || !sel.is_empty(),
        paths: sel,
        project: None,
        state_only: false,
    };
    let report = crate::archive::run_export(ctx, &list, &opts)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("{}", t!("export.done", path = report.archive.as_str()));
    println!(
        "{}",
        t!(
            "export.summary",
            agents = report.agents.len(),
            files = report.files,
            databases = report.databases,
            bytes = report.bytes,
            excluded = report.excluded
        )
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_import(
    ctx: &Ctx,
    archive: &Path,
    rebase: &[String],
    agents: Option<&str>,
    on_conflict: &str,
    allow_missing: bool,
    dry_run: bool,
    yes: bool,
    backup_dir: Option<PathBuf>,
    json: bool,
) -> Result<()> {
    let policy = match on_conflict {
        "skip" => crate::archive::Policy::Skip,
        "replace" => crate::archive::Policy::Replace,
        other => bail!("{}", t!("import.err_policy", policy = other)),
    };
    let rules = crate::spec::prepare_rules(rebase)?;
    let names: Option<Vec<String>> =
        agents.map(|a| a.split(',').map(|s| s.trim().to_string()).collect());
    let list = adapters::get_adapters(names.as_deref())?;
    let staging = crate::archive::open(archive)?;
    let missing = crate::archive::verify_paths(&staging.manifest.paths, &rules);
    if !missing.is_empty() {
        println!("{}", t!("import.missing_paths", count = missing.len()));
        for p in missing.iter().take(10) {
            println!("  {}", p);
        }
        if !allow_missing && !yes {
            println!("{}", t!("common.aborted"));
            return Ok(());
        }
    }
    if !yes && !dry_run {
        print!(
            "{}",
            t!(
                "import.confirm",
                archive = archive.display().to_string().as_str(),
                agents = staging.manifest.agents.len()
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
    let bdir = backup_dir.unwrap_or_else(|| ctx.default_backup_dir());
    let spec = match rules.first() {
        Some((o, n)) => ReplaceSpec::new(o, n)?,
        None => ReplaceSpec::identity(),
    };
    let journal_agents = staging.manifest.agents.clone();
    let mut backup = backup::Backup::new(&bdir, &spec, journal_agents, dry_run);
    let opts = crate::archive::ImportOpts {
        rules: rules.clone(),
        agents: names.clone(),
        policy,
        allow_missing,
        dry_run,
    };
    // save the journal even when the import fails midway: everything
    // written so far must stay reversible
    let report = match crate::archive::run_import(ctx, &staging, &list, &opts, &mut backup) {
        Ok(r) => r,
        Err(e) => {
            let _ = backup.save();
            return Err(e);
        }
    };
    backup.save()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!(
        "{}",
        t!(
            "import.summary",
            placed = report.placed + report.merged,
            replaced = report.replaced,
            skipped = report.skipped,
            changes = report.changes
        )
    );
    if dry_run {
        println!("{}", t!("mv.dry_run"));
    } else {
        println!("{}: movara undo --id {}", t!("mv.undo"), backup.manifest.id);
    }
    Ok(())
}

// ------------------------------------------------------- move / receive

fn shell_quote(v: &str) -> String {
    format!("'{}'", v.replace('\'', "'\\''"))
}

fn ssh_factory(host: &str) -> impl Fn(&str, bool) -> std::process::Command + '_ {
    move |dst: &str, plan_only: bool| {
        let mut c = std::process::Command::new("ssh");
        c.arg("-T").arg(host).arg("movara").arg("receive");
        c.arg("--dst").arg(shell_quote(dst));
        if plan_only {
            c.arg("--plan-only");
        } else {
            c.arg("--yes");
        }
        c
    }
}

pub struct MoveOutcome {
    pub success: bool,
    pub report_json: Option<serde_json::Value>,
}

/// the move core, transport-injected: `factory` spawns `movara receive`
/// on the target (ssh in production, the local binary in tests)
#[allow(clippy::too_many_arguments)]
pub fn perform_move<'f>(
    ctx: &Ctx,
    src: &Path,
    dst_str: &str,
    factory: &'f (dyn for<'a> Fn(&'a str, bool) -> std::process::Command + 'f),
    state_only: bool,
    cleanup: bool,
    dry_run: bool,
    yes: bool,
    agents: Option<&str>,
) -> Result<MoveOutcome> {
    let src_abs = crate::spec::absolutish(src);
    if !src_abs.is_dir() {
        bail!(
            "{}",
            t!("mv.err_not_dir", src = src.display().to_string().as_str())
        );
    }
    let names: Option<Vec<String>> =
        agents.map(|a| a.split(',').map(|s| s.trim().to_string()).collect());
    let list = adapters::get_adapters(names.as_deref())?;
    if !dry_run {
        live_gate(&list, yes)?;
    }

    // preflight A: what on THIS host references the source
    let probe = ReplaceSpec::new(&src_abs.to_string_lossy(), &src_abs.to_string_lossy())?;
    let refs: usize = list.iter().map(|a| a.scan(ctx, &probe).len()).sum();
    println!(
        "{}",
        t!(
            "move.preflight_local",
            agents = list.len(),
            refs = refs,
            src = src_abs.display().to_string().as_str()
        )
    );

    // preflight B: destination free + receive reachable on the target
    let plan = factory(dst_str, true)
        .output()
        .map_err(|e| anyhow::anyhow!("{}: {}", t!("move.err_transport"), e))?;
    if !plan.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&plan.stderr));
        // ssh itself fails with 255; anything else is receive's verdict
        if plan.status.code() == Some(255) {
            bail!("{}", t!("move.err_transport"));
        }
        bail!("{}", t!("move.err_preflight"));
    }
    if dry_run {
        println!("{}", t!("mv.dry_run"));
        return Ok(MoveOutcome {
            success: true,
            report_json: None,
        });
    }
    if !yes {
        print!(
            "{}",
            t!(
                "move.confirm",
                src = src_abs.display().to_string().as_str(),
                dst = dst_str
            )
        );
        let _ = std::io::stdout().flush();
        let mut ans = String::new();
        std::io::stdin().read_line(&mut ans)?;
        if !ans.trim().eq_ignore_ascii_case("y") && !ans.trim().eq_ignore_ascii_case("yes") {
            println!("{}", t!("common.aborted"));
            return Ok(MoveOutcome {
                success: false,
                report_json: None,
            });
        }
    }

    // stream the archive into the target's receive
    let mut child = factory(dst_str, false)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("{}: {}", t!("move.err_transport"), e))?;
    let stdin = child.stdin.take().context("receive stdin")?;
    let writer = crate::archive::ArchiveWriter::from_writer(stdin);
    let opts = crate::archive::ExportOpts {
        out: PathBuf::new(),
        filtered: true,
        paths: vec![crate::ctx::path_str(&src_abs)],
        project: Some(src_abs.clone()),
        state_only,
    };
    let export = crate::archive::run_export_into(ctx, &list, &opts, writer, None)?;
    for sec in &export.secrets {
        eprintln!("{}: {}", t!("move.secret_warning"), sec);
    }

    let out = child.wait_with_output().context("receive wait")?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let report_json = stdout
        .lines()
        .rev()
        .find(|l| l.starts_with("movara-report: "))
        .and_then(|l| serde_json::from_str(l.trim_start_matches("movara-report: ")).ok());
    let success = out.status.success() && report_json.is_some();
    if !success {
        eprintln!("{}", String::from_utf8_lossy(&out.stderr));
        if out.status.code() == Some(255) {
            bail!("{}", t!("move.err_transport"));
        }
        bail!("{}", t!("move.err_remote"));
    }
    println!("{}", t!("move.summary", dst = dst_str));

    // source cleanup: structured report + explicit flag; shared members
    // (databases, projection carriers) are never deleted (see
    // archive::cleanup_source)
    if !cleanup {
        println!(
            "{}",
            t!(
                "move.cleanup_hint",
                src = src_abs.display().to_string().as_str(),
                dst = dst_str
            )
        );
    }
    if cleanup {
        let mut bk = backup::Backup::new(
            &ctx.default_backup_dir(),
            &ReplaceSpec::identity(),
            export.agents.clone(),
            false,
        );
        bk.save()?;
        let rep = match crate::archive::cleanup_source(ctx, &export.members, &mut bk) {
            Ok(r) => r,
            Err(e) => {
                let _ = bk.save();
                return Err(e);
            }
        };
        bk.save()?;
        println!(
            "{}",
            t!(
                "cleanup.summary",
                removed = rep.removed.len(),
                kept = rep.kept_shared.len(),
                id = bk.manifest.id.as_str()
            )
        );
    }
    Ok(MoveOutcome {
        success,
        report_json,
    })
}

#[allow(clippy::too_many_arguments)]
fn cmd_move(
    ctx: &Ctx,
    src: &Path,
    dest: &str,
    state_only: bool,
    cleanup: bool,
    dry_run: bool,
    yes: bool,
    agents: Option<&str>,
) -> Result<()> {
    let (host, dst) = dest
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("{}: {}", t!("move.err_dest"), dest))?;
    if host.is_empty() || dst.is_empty() {
        bail!("{}: {}", t!("move.err_dest"), dest);
    }
    let dst_owned = dst.to_string();
    let factory = ssh_factory(host);
    perform_move(
        ctx, src, &dst_owned, &factory, state_only, cleanup, dry_run, yes, agents,
    )?;
    Ok(())
}

fn cmd_receive(ctx: &Ctx, dst: &Path, plan_only: bool, yes: bool, json: bool) -> Result<()> {
    let dst_abs = crate::spec::absolutish(dst);
    if plan_only {
        let installed: Vec<String> = adapters::all()
            .iter()
            .filter(|a| a.installed(ctx))
            .map(|a| a.name().to_string())
            .collect();
        let available = crate::archive::dst_available(&dst_abs);
        if json {
            println!(
                "{}",
                serde_json::json!({ "dst": dst_abs, "available": available, "agents": installed })
            );
        } else {
            let verdict = if available {
                t!("receive.plan_ok")
            } else {
                t!("receive.plan_conflict_short")
            };
            println!("{} {}", t!("receive.plan_dst"), verdict);
        }
        if !available {
            bail!(
                "{}",
                t!(
                    "receive.plan_conflict",
                    dst = dst_abs.display().to_string().as_str()
                )
            );
        }
        return Ok(());
    }
    if !yes {
        bail!("{}", t!("receive.err_yes"));
    }
    use std::io::IsTerminal as _;
    if std::io::stdin().is_terminal() {
        bail!("{}", t!("receive.err_tty"));
    }
    // the landing side has the same live-agent hazard as the sending
    // side: a running agent here will rewrite what just landed
    let gate_list = adapters::get_adapters(None)?;
    live_gate(&gate_list, true)?;
    // re-check on the target before the stream lands anything
    if !crate::archive::dst_available(&dst_abs) {
        bail!(
            "{}",
            t!(
                "receive.plan_conflict",
                dst = dst_abs.display().to_string().as_str()
            )
        );
    }
    let doc = receive_core(ctx, &dst_abs, std::io::stdin().lock())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&doc)?);
    } else {
        println!(
            "{}",
            t!(
                "receive.summary",
                project = doc["project_files"].as_u64().unwrap_or(0),
                placed = doc["placed"].as_u64().unwrap_or(0) + doc["merged"].as_u64().unwrap_or(0),
                skipped = doc["skipped"].as_u64().unwrap_or(0)
            )
        );
        println!(
            "{}: movara undo --id {}",
            t!("mv.undo"),
            doc["undo_id"].as_str().unwrap_or_default()
        );
    }
    // the machine-readable verdict line the move side parses
    println!("movara-report: {}", serde_json::to_string(&doc)?);
    Ok(())
}

/// the receive core, reader-injected (stdin in production, a file or
/// cursor in tests): extract the WHOLE stream first, place the project,
/// verify, import the state with the auto rule, and return the
/// structured report document
pub fn receive_core<R: std::io::Read>(
    ctx: &Ctx,
    dst_abs: &Path,
    reader: R,
) -> Result<serde_json::Value> {
    let staging = crate::archive::open_stream(reader)?;
    // the auto rule: project source path -> destination on THIS host;
    // a same-path move is the defined zero-pair verbatim import
    let src_path = staging
        .manifest
        .project
        .as_ref()
        .map(|p| p.source_path.clone())
        .unwrap_or_default();
    let dst_s = crate::ctx::path_str(dst_abs);
    let rules = if src_path.is_empty() || src_path == dst_s {
        vec![]
    } else {
        crate::spec::prepare_rules(&[format!("{}:{}", src_path, dst_s)])?
    };
    let spec = match rules.first() {
        Some((o, n)) => ReplaceSpec::new(o, n)?,
        None => ReplaceSpec::identity(),
    };
    let mut backup = backup::Backup::new(
        &ctx.default_backup_dir(),
        &spec,
        staging.manifest.agents.clone(),
        false,
    );
    // the journal exists on disk BEFORE the first placement, so even a
    // kill mid-placement leaves a reversible record
    backup.save()?;
    let mut placement = crate::archive::PlacementReport::default();
    let placed = (|| -> Result<()> {
        crate::archive::place_project(&staging, dst_abs, &mut backup, &mut placement)?;
        crate::archive::place_project_memory(&staging, dst_abs, &mut backup, &mut placement)
    })();
    // re-save right after the placements so even a kill or an error
    // below leaves a journal that reverses everything done so far
    backup.save()?;
    placed?;
    if !placement.refused.is_empty() {
        for r in &placement.refused {
            eprintln!("{}: {}", t!("receive.refused"), r);
        }
        bail!("{}", t!("receive.err_conflict"));
    }
    let missing = crate::archive::verify_paths(&staging.manifest.paths, &rules);
    if !missing.is_empty() {
        for m in &missing {
            eprintln!("  {}", m);
        }
        bail!("{}", t!("receive.err_missing_paths", count = missing.len()));
    }
    let list = adapters::get_adapters(None)?;
    let report = match crate::archive::run_import(
        ctx,
        &staging,
        &list,
        &crate::archive::ImportOpts {
            rules: rules.clone(),
            agents: None,
            policy: crate::archive::Policy::Skip,
            allow_missing: true,
            dry_run: false,
        },
        &mut backup,
    ) {
        Ok(r) => r,
        Err(e) => {
            let _ = backup.save();
            return Err(e);
        }
    };
    backup.save()?;
    Ok(serde_json::json!({
        "project_files": placement.project_files,
        "memory_files": placement.memory_files,
        "placed": report.placed,
        "replaced": report.replaced,
        "skipped": report.skipped,
        "skipped_shared_db": report.skipped_shared_dbs,
        "merged": report.merged,
        "changes": report.changes,
        "missing_paths": missing,
        "rules": rules,
        "undo_id": backup.manifest.id,
    }))
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
/// live-agent gate for every mutating command: a running agent holds
/// its state registry in memory and re-persists it after the rewrite
/// (observed with Kimi Code's background server re-creating a renamed
/// session bucket). Refused without --yes; with --yes a loud warning
/// and the run proceeds.
fn live_gate(list: &[Box<dyn Adapter>], yes: bool) -> Result<()> {
    let live = adapters::live_agent_processes(list);
    if live.is_empty() {
        return Ok(());
    }
    let names = live.join(", ");
    if !yes {
        bail!("{}", t!("mv.err_live_agents", agents = names.as_str()));
    }
    eprintln!("{}", t!("mv.warn_live_agents", agents = names.as_str()));
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
    if !dry_run {
        live_gate(&adapters, yes)?;
    }
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
    if !dry_run {
        live_gate(&adapters, yes)?;
    }
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

// ------------------------------------------------------------------ sync

/// the remote half of the sync protocol, transport-injected: ssh in
/// production (SshSyncTransport below), the second Ctx in-process in
/// tests — the same seam receive_core uses for `movara move`
pub trait SyncTransport {
    fn probe(
        &self,
        project: &str,
        freshness: u64,
        agents: Option<&str>,
    ) -> Result<sync::ProbeReport>;
    fn inventory(
        &self,
        project: &str,
        other: &str,
        pair_id: &str,
        agents: Option<&str>,
    ) -> Result<BTreeMap<String, sync::MemberInv>>;
    /// raw bytes of the given members from the other host
    fn pack(
        &self,
        project: &str,
        members: &[String],
        agents: Option<&str>,
    ) -> Result<sync::Staging>;
    #[allow(clippy::too_many_arguments)]
    fn apply(
        &self,
        project: &str,
        other: &str,
        pair_id: &str,
        run_id: &str,
        side_a: bool,
        plan: &[sync::PlannedMember],
        staging: &sync::Staging,
        agents: Option<&str>,
    ) -> Result<sync::ApplyReport>;
    /// persist the authoritative ledger on the other host
    fn commit(&self, pair_id: &str, ledger: &sync::LedgerState) -> Result<()>;
}

/// runs `movara sync-agent <step>` on the other host over ssh
struct SshSyncTransport<'a> {
    host: &'a str,
}

fn json_over_ssh(
    cmd: &mut std::process::Command,
    stdin: Option<&[u8]>,
) -> Result<serde_json::Value> {
    use std::process::Stdio;
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let out = match stdin {
        None => cmd.output(),
        Some(payload) => {
            let mut child = cmd.spawn().context("spawn ssh")?;
            child
                .stdin
                .take()
                .context("ssh stdin")?
                .write_all(payload)?;
            // drop our handle so the remote sees EOF, then collect
            drop(child.stdin.take());
            child.wait_with_output()
        }
    }?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        bail!("{}", t!("sync.err_transport", err = err.as_str()));
    }
    Ok(serde_json::from_slice(&out.stdout)?)
}

impl SyncTransport for SshSyncTransport<'_> {
    fn probe(
        &self,
        project: &str,
        freshness: u64,
        agents: Option<&str>,
    ) -> Result<sync::ProbeReport> {
        let mut c = std::process::Command::new("ssh");
        c.arg("-T")
            .arg(self.host)
            .arg("movara")
            .arg("sync-agent")
            .arg("probe")
            .arg("--project")
            .arg(shell_quote(project))
            .arg("--freshness")
            .arg(freshness.to_string());
        if let Some(a) = agents {
            c.arg("--agents").arg(shell_quote(a));
        }
        Ok(serde_json::from_value(json_over_ssh(&mut c, None)?)?)
    }

    fn inventory(
        &self,
        project: &str,
        other: &str,
        pair_id: &str,
        agents: Option<&str>,
    ) -> Result<BTreeMap<String, sync::MemberInv>> {
        let mut c = std::process::Command::new("ssh");
        c.arg("-T")
            .arg(self.host)
            .arg("movara")
            .arg("sync-agent")
            .arg("inventory")
            .arg("--project")
            .arg(shell_quote(project))
            .arg("--other")
            .arg(shell_quote(other))
            .arg("--pair")
            .arg(shell_quote(pair_id));
        if let Some(a) = agents {
            c.arg("--agents").arg(shell_quote(a));
        }
        Ok(serde_json::from_value(json_over_ssh(&mut c, None)?)?)
    }

    fn pack(
        &self,
        project: &str,
        members: &[String],
        agents: Option<&str>,
    ) -> Result<sync::Staging> {
        let mut c = std::process::Command::new("ssh");
        c.arg("-T")
            .arg(self.host)
            .arg("movara")
            .arg("sync-agent")
            .arg("pack")
            .arg("--project")
            .arg(shell_quote(project));
        if let Some(a) = agents {
            c.arg("--agents").arg(shell_quote(a));
        }
        let payload = serde_json::to_vec(members)?;
        let v = json_over_ssh(&mut c, Some(&payload))?;
        use base64::Engine as _;
        let mut out = sync::Staging::new();
        for (m, b64) in v.as_object().context("pack payload")? {
            let s = b64.as_str().context("pack: non-string entry")?;
            out.insert(
                m.clone(),
                base64::engine::general_purpose::STANDARD.decode(s)?,
            );
        }
        Ok(out)
    }

    fn apply(
        &self,
        project: &str,
        other: &str,
        pair_id: &str,
        run_id: &str,
        side_a: bool,
        plan: &[sync::PlannedMember],
        staging: &sync::Staging,
        agents: Option<&str>,
    ) -> Result<sync::ApplyReport> {
        use base64::Engine as _;
        let mut wire_staging = serde_json::Map::new();
        for (m, bytes) in staging {
            wire_staging.insert(
                m.clone(),
                serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(bytes)),
            );
        }
        let payload = serde_json::to_vec(&serde_json::json!({
            "plan": plan,
            "staging": wire_staging,
        }))?;
        let mut c = std::process::Command::new("ssh");
        c.arg("-T")
            .arg(self.host)
            .arg("movara")
            .arg("sync-agent")
            .arg("apply")
            .arg("--project")
            .arg(shell_quote(project))
            .arg("--other")
            .arg(shell_quote(other))
            .arg("--pair")
            .arg(shell_quote(pair_id))
            .arg("--side")
            .arg(if side_a { "a" } else { "b" })
            .arg("--run")
            .arg(shell_quote(run_id));
        if let Some(a) = agents {
            c.arg("--agents").arg(shell_quote(a));
        }
        Ok(serde_json::from_value(json_over_ssh(
            &mut c,
            Some(&payload),
        )?)?)
    }

    fn commit(&self, pair_id: &str, ledger: &sync::LedgerState) -> Result<()> {
        let payload = serde_json::to_vec(ledger)?;
        let mut c = std::process::Command::new("ssh");
        c.arg("-T")
            .arg(self.host)
            .arg("movara")
            .arg("sync-agent")
            .arg("commit")
            .arg("--pair")
            .arg(shell_quote(pair_id));
        json_over_ssh(&mut c, Some(&payload))?;
        Ok(())
    }
}

pub struct SyncOpts {
    pub freshness: u64,
    pub dry_run: bool,
    pub backup_dir: Option<PathBuf>,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct SyncCounts {
    pub ff_to_a: usize,
    pub ff_to_b: usize,
    pub copy_to_a: usize,
    pub copy_to_b: usize,
    pub conflicts: usize,
    pub noop: usize,
}

#[derive(Debug)]
pub struct SyncOutcome {
    pub counts: SyncCounts,
    pub a: sync::ApplyReport,
    pub b: sync::ApplyReport,
    pub backup_id: Option<String>,
}

fn plan_counts(plan: &[sync::PlannedMember]) -> SyncCounts {
    let mut c = SyncCounts::default();
    for pm in plan {
        match &pm.action {
            sync::Action::FastForwardToA { .. } => c.ff_to_a += 1,
            sync::Action::FastForwardToB { .. } => c.ff_to_b += 1,
            sync::Action::CopyToA => c.copy_to_a += 1,
            sync::Action::CopyToB => c.copy_to_b += 1,
            sync::Action::ConflictWinnerA { .. } => c.conflicts += 1,
            _ => c.noop += 1,
        }
    }
    c
}

fn probe_detail(p: &sync::ProbeReport) -> String {
    let mut items = p.processes.clone();
    items.extend(
        p.fresh_wal
            .iter()
            .map(|w| w.rsplit('/').next().unwrap_or(w).to_string()),
    );
    items.extend(
        p.fresh_members
            .iter()
            .map(|m| m.rsplit('/').next().unwrap_or(m).to_string()),
    );
    items.dedup();
    items.join(", ")
}

/// the sync core, transport-injected (ssh in production, in-process in
/// tests). Order: live-gate BOTH hosts, take the lease, inventory both,
/// plan once, stage bytes both directions, apply A then B, commit A's
/// authoritative ledger to both. A member that changed between inventory
/// and apply is skipped by the pre-state guard and its base does not
/// advance — the next run re-plans it from the old base.
pub fn perform_sync(
    ctx: &Ctx,
    pair: &sync::Pair,
    list: &[Box<dyn Adapter>],
    tr: &dyn SyncTransport,
    opts: &SyncOpts,
) -> Result<SyncOutcome> {
    let agents_arg: Option<String> = pair.agents.as_ref().map(|a| a.join(","));
    let agents = agents_arg.as_deref();
    let pid = sync::pair_id(pair);

    // 1. live gates on both hosts — a hot host refuses the whole run
    let pa = sync::probe(ctx, list, &pair.local, opts.freshness);
    if pa.hot {
        bail!(
            "{}",
            t!("sync.err_hot_local", detail = probe_detail(&pa).as_str())
        );
    }
    let pb = tr.probe(&pair.remote_path, opts.freshness, agents)?;
    if pb.hot {
        bail!(
            "{}",
            t!(
                "sync.err_hot_remote",
                host = pair.remote_host.as_str(),
                detail = probe_detail(&pb).as_str()
            )
        );
    }

    // 2. the pair lease: a second initiator on this host is refused
    let lease = sync::Lease::acquire(ctx, &pid, 600)?
        .ok_or_else(|| anyhow::anyhow!("{}", t!("sync.err_lease_held")))?;
    let outcome = match sync_locked(ctx, pair, list, tr, agents, &pid, opts) {
        Ok(o) => o,
        Err(e) => {
            lease.release();
            return Err(e);
        }
    };
    lease.release();
    Ok(outcome)
}

#[allow(clippy::too_many_arguments)]
fn sync_locked(
    ctx: &Ctx,
    pair: &sync::Pair,
    list: &[Box<dyn Adapter>],
    tr: &dyn SyncTransport,
    agents: Option<&str>,
    pid: &str,
    opts: &SyncOpts,
) -> Result<SyncOutcome> {
    let mut ledger = sync::Ledger::load(ctx, pid)?;
    let inv_a = sync::inventory(ctx, list, &pair.local, &pair.remote_path, &ledger);
    let inv_b = tr.inventory(&pair.remote_path, &pair.local, pid, agents)?;
    let plan = sync::plan(&inv_a, &inv_b, &ledger.state);
    let counts = plan_counts(&plan);
    if opts.dry_run {
        return Ok(SyncOutcome {
            counts,
            a: sync::ApplyReport::default(),
            b: sync::ApplyReport::default(),
            backup_id: None,
        });
    }
    let run_id = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();

    // stage both directions: A needs B's bytes for its incoming rows,
    // B needs A's for its own (read straight off this host's state tree)
    let need_a: Vec<String> = plan
        .iter()
        .filter(|pm| {
            matches!(
                pm.action,
                sync::Action::FastForwardToA { .. } | sync::Action::CopyToA
            )
        })
        .map(|pm| pm.member.clone())
        .collect();
    let need_b: Vec<String> = plan
        .iter()
        .filter(|pm| {
            matches!(
                pm.action,
                sync::Action::FastForwardToB { .. }
                    | sync::Action::CopyToB
                    | sync::Action::ConflictWinnerA { .. }
            )
        })
        .map(|pm| pm.member.clone())
        .collect();
    let staging_a = tr.pack(&pair.remote_path, &need_a, agents)?;
    let mut staging_b = sync::Staging::new();
    for m in &need_b {
        if let Some(raw) = sync::read_member(ctx, list, m) {
            staging_b.insert(m.clone(), raw);
        }
    }

    // apply A (in-process), then B (over the transport)
    let mut backup = backup::Backup::new(
        &opts
            .backup_dir
            .clone()
            .unwrap_or_else(|| ctx.default_backup_dir()),
        &ReplaceSpec::new(&pair.remote_path, &pair.local)?,
        list.iter().map(|a| a.name().to_string()).collect(),
        false,
    );
    // the journal exists on disk BEFORE the first placement, exactly
    // like receive: a kill mid-apply leaves a reversible record
    backup.save()?;
    let ra = sync::apply_half(
        ctx,
        list,
        &pair.local,
        &pair.remote_path,
        &plan,
        true,
        &staging_a,
        &mut ledger.state,
        &mut backup,
        &run_id,
    )?;
    backup.save()?;
    let rb = tr.apply(
        &pair.remote_path,
        &pair.local,
        pid,
        &run_id,
        false,
        &plan,
        &staging_b,
        agents,
    )?;

    // commit: A computes the authoritative ledger (only rows the
    // receiving side actually applied may advance), then ships it to B
    let confirmed = sync::confirmed_members(&plan, &ra.applied, &rb.applied);
    sync::commit_plan(&mut ledger.state, &plan, &confirmed, &rb.siblings);
    ledger.save()?;
    tr.commit(pid, &ledger.state)?;

    Ok(SyncOutcome {
        counts,
        a: ra,
        b: rb,
        backup_id: Some(backup.manifest.id),
    })
}

fn cmd_sync(
    ctx: &Ctx,
    name: &str,
    freshness: u64,
    dry_run: bool,
    backup_dir: Option<PathBuf>,
    json: bool,
) -> Result<()> {
    let mut pair = sync::pair_get(ctx, name)?;
    pair.local = crate::ctx::path_str(&crate::spec::absolutish(Path::new(&pair.local)));
    if !Path::new(&pair.local).is_dir() {
        bail!("{}", t!("sync.err_no_local", path = pair.local.as_str()));
    }
    let names: Option<Vec<String>> = pair.agents.clone();
    let list = adapters::get_adapters(names.as_deref())?;
    let tr = SshSyncTransport {
        host: &pair.remote_host,
    };
    let out = perform_sync(
        ctx,
        &pair,
        &list,
        &tr,
        &SyncOpts {
            freshness,
            dry_run,
            backup_dir,
        },
    )?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "pair": pair.name,
                "dryRun": dry_run,
                "counts": out.counts,
                "a": out.a,
                "b": out.b,
                "backupId": out.backup_id,
            }))?
        );
    } else {
        println!(
            "{}",
            t!(
                "sync.plan_summary",
                ff_a = out.counts.ff_to_a,
                ff_b = out.counts.ff_to_b,
                copy_ab = out.counts.copy_to_b,
                copy_ba = out.counts.copy_to_a,
                conflict = out.counts.conflicts,
                noop = out.counts.noop
            )
        );
        if dry_run {
            println!("{}", t!("mv.dry_run"));
        } else {
            let a = &out.a;
            let b = &out.b;
            println!(
                "{}",
                t!(
                    "sync.summary",
                    placed = a.placed + b.placed,
                    replaced = a.replaced + b.replaced,
                    conflicts = a.conflicts_local_kept + b.conflicts_local_kept,
                    skipped = a.skipped + b.skipped
                )
            );
            if let Some(id) = &out.backup_id {
                println!("{}: movara undo --id {}", t!("mv.undo"), id);
            }
        }
    }
    Ok(())
}

fn cmd_sync_pair(ctx: &Ctx, cmd: PairCmd) -> Result<()> {
    match cmd {
        PairCmd::Add {
            name,
            local,
            host,
            remote,
            with_files,
            agents,
        } => {
            let p = sync::Pair {
                name: name.clone(),
                local: crate::ctx::path_str(&crate::spec::absolutish(&local)),
                remote_host: host.clone(),
                remote_path: remote.clone(),
                with_files,
                agents: agents.map(|a| a.split(',').map(|s| s.trim().to_string()).collect()),
            };
            sync::pair_add(ctx, &p)?;
            println!(
                "{}",
                t!(
                    "sync.pair_added",
                    name = name.as_str(),
                    local = p.local.as_str(),
                    host = host.as_str(),
                    remote = p.remote_path.as_str()
                )
            );
            Ok(())
        }
        PairCmd::List { json } => {
            let pairs = sync::pair_list(ctx)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&pairs)?);
                return Ok(());
            }
            if pairs.is_empty() {
                println!("{}", t!("sync.pairs_none"));
                return Ok(());
            }
            for p in pairs {
                println!(
                    "{}",
                    t!(
                        "sync.pair_row",
                        name = p.name.as_str(),
                        local = p.local.as_str(),
                        host = p.remote_host.as_str(),
                        remote = p.remote_path.as_str()
                    )
                );
            }
            Ok(())
        }
        PairCmd::Remove { name } => {
            if sync::pair_remove(ctx, &name)? {
                println!("{}", t!("sync.pair_removed", name = name.as_str()));
                Ok(())
            } else {
                bail!("{}", t!("sync.err_unknown_pair", name = name.as_str()));
            }
        }
    }
}

fn cmd_sync_status(ctx: &Ctx, name: &str, json: bool) -> Result<()> {
    let pair = sync::pair_get(ctx, name)?;
    let ledger = sync::Ledger::load(ctx, &sync::pair_id(&pair))?;
    let siblings = ledger
        .state
        .members
        .values()
        .filter(|r| r.sibling.is_some())
        .count();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "name": pair.name,
                "local": pair.local,
                "remoteHost": pair.remote_host,
                "remotePath": pair.remote_path,
                "withFiles": pair.with_files,
                "lastSync": ledger.state.last_sync,
                "members": ledger.state.members.len(),
                "siblings": siblings,
            }))?
        );
        return Ok(());
    }
    match &ledger.state.last_sync {
        None => println!(
            "{}",
            t!(
                "sync.status_none",
                name = pair.name.as_str(),
                local = pair.local.as_str(),
                host = pair.remote_host.as_str(),
                remote = pair.remote_path.as_str()
            )
        ),
        Some(when) => println!(
            "{}",
            t!(
                "sync.status_line",
                name = pair.name.as_str(),
                local = pair.local.as_str(),
                host = pair.remote_host.as_str(),
                remote = pair.remote_path.as_str(),
                when = when.as_str(),
                members = ledger.state.members.len(),
                siblings = siblings
            )
        ),
    }
    Ok(())
}

fn agent_adapters(agents: Option<&str>) -> Result<Vec<Box<dyn Adapter>>> {
    let names: Option<Vec<String>> =
        agents.map(|a| a.split(',').map(|s| s.trim().to_string()).collect());
    adapters::get_adapters(names.as_deref())
}

fn cmd_sync_agent(ctx: &Ctx, cmd: AgentCmd) -> Result<()> {
    match cmd {
        AgentCmd::Probe {
            project,
            freshness,
            agents,
        } => {
            let list = agent_adapters(agents.as_deref())?;
            let rep = sync::probe(ctx, &list, &project, freshness);
            println!("{}", serde_json::to_string_pretty(&rep)?);
            Ok(())
        }
        AgentCmd::Inventory {
            project,
            other,
            pair,
            agents,
        } => {
            let list = agent_adapters(agents.as_deref())?;
            let ledger = sync::Ledger::load(ctx, &pair)?;
            let inv = sync::inventory(ctx, &list, &project, &other, &ledger);
            println!("{}", serde_json::to_string_pretty(&inv)?);
            Ok(())
        }
        AgentCmd::Pack { project: _, agents } => {
            let list = agent_adapters(agents.as_deref())?;
            let members: Vec<String> = serde_json::from_reader(std::io::stdin())?;
            use base64::Engine as _;
            let mut out = serde_json::Map::new();
            for m in &members {
                if let Some(raw) = sync::read_member(ctx, &list, m) {
                    out.insert(
                        m.clone(),
                        serde_json::Value::String(
                            base64::engine::general_purpose::STANDARD.encode(raw),
                        ),
                    );
                }
            }
            println!("{}", serde_json::Value::Object(out));
            Ok(())
        }
        AgentCmd::Apply {
            project,
            other,
            pair,
            side,
            run,
            agents,
        } => {
            if side != "a" && side != "b" {
                bail!("{}", t!("sync.err_side"));
            }
            let list = agent_adapters(agents.as_deref())?;
            let input: serde_json::Value = serde_json::from_reader(std::io::stdin())?;
            let plan: Vec<sync::PlannedMember> =
                serde_json::from_value(input.get("plan").cloned().unwrap_or_default())?;
            use base64::Engine as _;
            let mut staging = sync::Staging::new();
            if let Some(ws) = input.get("staging").and_then(|v| v.as_object()) {
                for (m, b64) in ws {
                    let s = b64.as_str().context("staging: non-string entry")?;
                    staging.insert(
                        m.clone(),
                        base64::engine::general_purpose::STANDARD.decode(s)?,
                    );
                }
            }
            let mut ledger = sync::Ledger::load(ctx, &pair)?;
            let mut backup = backup::Backup::new(
                &ctx.default_backup_dir(),
                &ReplaceSpec::new(&other, &project)?,
                list.iter().map(|a| a.name().to_string()).collect(),
                false,
            );
            backup.save()?;
            let rep = sync::apply_half(
                ctx,
                &list,
                &project,
                &other,
                &plan,
                side == "a",
                &staging,
                &mut ledger.state,
                &mut backup,
                &run,
            );
            // persist sibling records + the journal even on partial
            // failure: whatever was applied must stay reversible
            ledger.save()?;
            backup.save()?;
            println!("{}", serde_json::to_string_pretty(&rep?)?);
            Ok(())
        }
        AgentCmd::Commit { pair } => {
            let state: sync::LedgerState = serde_json::from_reader(std::io::stdin())?;
            sync::Ledger {
                dir: ctx.home.join(".movara").join("sync").join(&pair),
                state,
            }
            .save()?;
            println!("{{\"ok\":true}}");
            Ok(())
        }
    }
}
