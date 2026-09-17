// SPDX-License-Identifier: GPL-3.0-or-later
//! rust-i18n embeds locales/*.yml at compile time, but cargo does not track
//! those files as sources — force a rebuild whenever they change.

fn main() {
    println!("cargo:rerun-if-changed=locales");
}
