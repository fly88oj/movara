// SPDX-License-Identifier: GPL-3.0-or-later
//! Small rusqlite helpers. All adapter SQL lives in the adapter modules as
//! inline literals executed with bound parameters only.

use anyhow::Result;
use rusqlite::Connection;
use rust_i18n::t;
use std::path::Path;
use std::time::Duration;

pub fn open_ro(db: &Path) -> Result<Connection> {
    let con = Connection::open_with_flags(
        db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    con.busy_timeout(Duration::from_secs(30))?;
    Ok(con)
}

/// open for writing; warn when a recently-touched -wal sidecar suggests the
/// owning agent may still be running
pub fn open_rw(db: &Path) -> Result<Connection> {
    if wal_recent(db) {
        eprintln!(
            "{}",
            t!("sqlite.wal_active", db = &db.display().to_string())
        );
    }
    let con = Connection::open(db)?;
    con.busy_timeout(Duration::from_secs(30))?;
    Ok(con)
}

/// a -wal sidecar modified within the last 10 minutes: the database is (or
/// very recently was) in use by another process
pub fn wal_recent(db: &Path) -> bool {
    let s = db.to_string_lossy().into_owned();
    let wal = std::path::PathBuf::from(format!("{}-wal", s));
    match std::fs::metadata(&wal) {
        Ok(meta) => meta
            .modified()
            .ok()
            .and_then(|m| m.elapsed().ok())
            .map(|age| age < Duration::from_secs(600))
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// checkpoint WAL so a plain file copy of the db is complete
pub fn checkpoint(db: &Path) -> Result<()> {
    let con = Connection::open(db)?;
    con.busy_timeout(Duration::from_secs(30))?;
    let _ = con.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
    Ok(())
}

/// purge replaced strings left in free pages so the file bytes match the
/// logical state
pub fn vacuum(con: &Connection) -> Result<()> {
    let _ = con.execute_batch("VACUUM");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn wal_recent_detects_fresh_and_stale_sidecars() {
        let dir = std::env::temp_dir().join(format!("ap-wal-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("x.db");
        std::fs::write(&db, b"db").unwrap();

        // no sidecar -> false
        assert!(!wal_recent(&db));

        // fresh sidecar -> true
        let wal = dir.join("x.db-wal");
        std::fs::write(&wal, b"w").unwrap();
        assert!(wal_recent(&db));

        // stale sidecar (mtime 1 hour ago) -> false
        let old = SystemTime::now() - Duration::from_secs(3600);
        let f = std::fs::File::options().write(true).open(&wal).unwrap();
        f.set_modified(old).unwrap();
        drop(f);
        assert!(!wal_recent(&db));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
