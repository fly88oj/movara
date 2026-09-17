// SPDX-License-Identifier: GPL-3.0-or-later
//! movara: move a project directory and rewrite every AI agent's local
//! session/config references to it, in one step (`movara mv`).
fn main() -> anyhow::Result<()> {
    movara::cli::run()
}
