use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::repository::Repository;

const RESET_JOURNAL_VERSION: u32 = 1;
const STAGING_PREFIX: &str = ".accordmesh-reset-staging-";
const BACKUP_PREFIX: &str = ".accordmesh-reset-backup-";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ResetPhase {
    Prepared,
    Swapped,
}

#[derive(Debug, Serialize, Deserialize)]
struct ResetJournal {
    version: u32,
    data_dir_name: String,
    staging_name: String,
    backup_name: String,
    phase: ResetPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetError {
    Preserved,
    RecoveryRequired,
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn parent_and_name(data_dir: &Path) -> io::Result<(PathBuf, String)> {
    let parent = data_dir
        .parent()
        .ok_or_else(|| invalid_data("reset data directory has no parent"))?
        .to_path_buf();
    let name = data_dir
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid_data("reset data directory has no valid name"))?
        .to_owned();
    Ok((parent, name))
}

fn journal_path(data_dir: &Path, phase: ResetPhase) -> io::Result<PathBuf> {
    let (parent, name) = parent_and_name(data_dir)?;
    let phase_name = match phase {
        ResetPhase::Prepared => "prepared",
        ResetPhase::Swapped => "swapped",
    };
    Ok(parent.join(format!(".{name}.reset-journal.{phase_name}.json")))
}

fn journal_paths(data_dir: &Path) -> io::Result<(PathBuf, PathBuf)> {
    Ok((
        journal_path(data_dir, ResetPhase::Prepared)?,
        journal_path(data_dir, ResetPhase::Swapped)?,
    ))
}

fn safe_sibling(parent: &Path, name: &str, prefix: &str) -> io::Result<PathBuf> {
    if !name.starts_with(prefix)
        || name.contains('/')
        || name.contains('\\')
        || name == "."
        || name == ".."
    {
        return Err(invalid_data("invalid reset journal path"));
    }
    let path = parent.join(name);
    if path.parent() != Some(parent) {
        return Err(invalid_data("reset path escaped parent"));
    }
    Ok(path)
}

fn path_present(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn remove_path(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path)
    } else {
        fs::remove_dir_all(path)
    }
}

fn sync_parent(parent: &Path) {
    #[cfg(unix)]
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
}

fn write_new_journal(path: &Path, journal: &ResetJournal) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid_data("reset journal has no parent"))?;
    fs::create_dir_all(parent)?;
    if path_present(path)? {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "reset journal already exists",
        ));
    }
    let temporary = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("reset-journal"),
        Uuid::new_v4()
    ));
    let result = (|| -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(journal)
            .map_err(|_| invalid_data("reset journal serialization failed"))?;
        {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        fs::rename(&temporary, path)?;
        sync_parent(parent);
        Ok(())
    })();
    if result.is_err() {
        let _ = remove_path(&temporary);
    }
    result
}

fn read_journal(path: &Path, expected_phase: ResetPhase) -> io::Result<ResetJournal> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid_data("reset journal is not a regular file"));
    }
    let bytes = fs::read(path)?;
    let journal: ResetJournal =
        serde_json::from_slice(&bytes).map_err(|_| invalid_data("reset journal is unreadable"))?;
    if journal.version != RESET_JOURNAL_VERSION {
        return Err(invalid_data("unsupported reset journal version"));
    }
    if journal.phase != expected_phase {
        return Err(invalid_data(
            "reset journal phase does not match its marker",
        ));
    }
    Ok(journal)
}

fn initialize_fresh_data_dir(path: &Path) -> io::Result<()> {
    remove_path(path)?;
    let repository = Repository::new(path.to_path_buf())?;
    repository.initialize().map_err(|_| {
        io::Error::new(
            io::ErrorKind::Other,
            "fresh repository initialization failed",
        )
    })
}

pub fn recovery_pending(data_dir: &Path) -> bool {
    journal_paths(data_dir)
        .and_then(|(prepared, swapped)| Ok(path_present(&prepared)? || path_present(&swapped)?))
        .unwrap_or(true)
}

pub fn recover_interrupted_reset(data_dir: &Path) -> io::Result<()> {
    let (prepared_path, swapped_path) = journal_paths(data_dir)?;
    let swapped_present = path_present(&swapped_path)?;
    let prepared_present = path_present(&prepared_path)?;
    let (journal_path, phase) = if swapped_present {
        (&swapped_path, ResetPhase::Swapped)
    } else if prepared_present {
        (&prepared_path, ResetPhase::Prepared)
    } else {
        return Ok(());
    };

    let journal = read_journal(journal_path, phase)?;
    let (parent, expected_name) = parent_and_name(data_dir)?;
    if journal.data_dir_name != expected_name {
        return Err(invalid_data("reset journal targets another data directory"));
    }

    let staging = safe_sibling(&parent, &journal.staging_name, STAGING_PREFIX)?;
    let backup = safe_sibling(&parent, &journal.backup_name, BACKUP_PREFIX)?;

    match phase {
        ResetPhase::Prepared => {
            if backup.exists() {
                if data_dir.exists() {
                    remove_path(data_dir)?;
                }
                fs::rename(&backup, data_dir)?;
            } else if !data_dir.exists() {
                return Err(invalid_data(
                    "prepared reset lost both active and backup data",
                ));
            }
            remove_path(&staging)?;
        }
        ResetPhase::Swapped => {
            if data_dir.exists() {
                remove_path(&backup)?;
                remove_path(&staging)?;
            } else if backup.exists() {
                fs::rename(&backup, data_dir)?;
                remove_path(&staging)?;
            } else {
                return Err(invalid_data(
                    "committed reset lost both active and backup data",
                ));
            }
        }
    }
    remove_path(&prepared_path)?;
    remove_path(&swapped_path)?;
    sync_parent(&parent);
    Ok(())
}

fn atomic_reset_data_dir_inner(
    data_dir: &Path,
    failpoint: Option<&'static str>,
) -> Result<(), ResetError> {
    if recover_interrupted_reset(data_dir).is_err() {
        return Err(ResetError::RecoveryRequired);
    }

    let (parent, data_dir_name) =
        parent_and_name(data_dir).map_err(|_| ResetError::RecoveryRequired)?;
    if fs::create_dir_all(&parent).is_err() {
        return Err(ResetError::Preserved);
    }

    let suffix = Uuid::new_v4().to_string();
    let staging_name = format!("{STAGING_PREFIX}{suffix}");
    let backup_name = format!("{BACKUP_PREFIX}{suffix}");
    let staging = parent.join(&staging_name);
    let backup = parent.join(&backup_name);
    let (prepared_path, swapped_path) = match journal_paths(data_dir) {
        Ok(value) => value,
        Err(_) => return Err(ResetError::RecoveryRequired),
    };

    if initialize_fresh_data_dir(&staging).is_err() {
        let _ = remove_path(&staging);
        return Err(ResetError::Preserved);
    }

    let mut journal = ResetJournal {
        version: RESET_JOURNAL_VERSION,
        data_dir_name,
        staging_name,
        backup_name,
        phase: ResetPhase::Prepared,
    };

    let mut committed = false;
    let operation = (|| -> io::Result<()> {
        write_new_journal(&prepared_path, &journal)?;
        if failpoint == Some("after_prepared") {
            return Err(io::Error::new(io::ErrorKind::Other, "test failpoint"));
        }

        if data_dir.exists() {
            fs::rename(data_dir, &backup)?;
        }
        if failpoint == Some("after_old_rename") {
            return Err(io::Error::new(io::ErrorKind::Other, "test failpoint"));
        }

        fs::rename(&staging, data_dir)?;
        if failpoint == Some("after_swap") {
            return Err(io::Error::new(io::ErrorKind::Other, "test failpoint"));
        }

        journal.phase = ResetPhase::Swapped;
        write_new_journal(&swapped_path, &journal)?;
        committed = true;
        if failpoint == Some("after_commit") {
            return Err(io::Error::new(io::ErrorKind::Other, "test failpoint"));
        }

        remove_path(&backup)?;
        remove_path(&prepared_path)?;
        remove_path(&swapped_path)?;
        sync_parent(&parent);
        Ok(())
    })();

    if operation.is_ok() {
        return Ok(());
    }

    let prepared_present = path_present(&prepared_path).unwrap_or(true);
    let swapped_present = path_present(&swapped_path).unwrap_or(true);
    if !prepared_present && !swapped_present {
        let _ = remove_path(&staging);
        return Err(ResetError::Preserved);
    }

    match recover_interrupted_reset(data_dir) {
        Ok(()) if committed => Ok(()),
        Ok(()) => Err(ResetError::Preserved),
        Err(_) => Err(ResetError::RecoveryRequired),
    }
}

pub fn atomic_reset_data_dir(data_dir: &Path) -> Result<(), ResetError> {
    atomic_reset_data_dir_inner(data_dir, None)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rusqlite::Connection;

    use super::*;

    fn test_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("accordmesh-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create test root");
        root
    }

    fn seed_old_data(data_dir: &Path) {
        let repository = Repository::new(data_dir.to_path_buf()).expect("create repository");
        repository.initialize().expect("initialize repository");
        fs::create_dir_all(data_dir.join("vault")).expect("create vault directory");
        fs::write(data_dir.join("vault/vault.json"), b"old-vault").expect("write vault marker");
        fs::create_dir_all(data_dir.join("credentials")).expect("create credentials directory");
        fs::write(data_dir.join("credentials/openai.enc"), b"encrypted-secret")
            .expect("write credential marker");
        fs::create_dir_all(data_dir.join("projects/project-1/runtime/realtime_pending"))
            .expect("create project runtime");
        fs::write(
            data_dir.join("projects/project-1/runtime/realtime_pending/chunk.enc"),
            b"encrypted-audio",
        )
        .expect("write chunk");
        fs::write(data_dir.join("old-sentinel"), b"preserve-or-delete").expect("write sentinel");
    }

    fn assert_fresh(data_dir: &Path) {
        assert!(data_dir.join("app.sqlite").exists());
        assert!(!data_dir.join("vault/vault.json").exists());
        assert!(!data_dir.join("credentials/openai.enc").exists());
        assert!(!data_dir.join("old-sentinel").exists());
        assert!(!data_dir.join("projects/project-1").exists());
        let connection =
            Connection::open(data_dir.join("app.sqlite")).expect("open fresh database");
        let projects: i64 = connection
            .query_row("select count(*) from projects", [], |row| row.get(0))
            .expect("count projects");
        assert_eq!(projects, 0);
    }

    fn assert_old_preserved(data_dir: &Path) {
        assert!(data_dir.join("vault/vault.json").exists());
        assert!(data_dir.join("credentials/openai.enc").exists());
        assert!(data_dir.join("old-sentinel").exists());
        assert!(data_dir
            .join("projects/project-1/runtime/realtime_pending/chunk.enc")
            .exists());
    }

    fn manual_journal(
        data_dir: &Path,
        staging_name: String,
        backup_name: String,
        phase: ResetPhase,
    ) {
        let journal = ResetJournal {
            version: RESET_JOURNAL_VERSION,
            data_dir_name: data_dir
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap()
                .to_owned(),
            staging_name,
            backup_name,
            phase,
        };
        write_new_journal(&journal_path(data_dir, phase).unwrap(), &journal)
            .expect("write journal");
    }

    #[test]
    fn atomic_reset_replaces_all_old_data_with_one_fresh_repository() {
        let root = test_root("reset-success");
        let data_dir = root.join("data");
        seed_old_data(&data_dir);

        atomic_reset_data_dir(&data_dir).expect("reset succeeds");
        assert_fresh(&data_dir);
        assert!(!recovery_pending(&data_dir));
        assert_eq!(fs::read_dir(&root).expect("read root").count(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn atomic_reset_removes_orphaned_data_without_a_vault_record() {
        let root = test_root("reset-orphaned");
        let data_dir = root.join("data");
        seed_old_data(&data_dir);
        fs::remove_file(data_dir.join("vault/vault.json")).expect("remove vault marker");

        atomic_reset_data_dir(&data_dir).expect("orphaned data reset succeeds");
        assert_fresh(&data_dir);
        assert!(!recovery_pending(&data_dir));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failure_before_swap_preserves_the_complete_old_state() {
        for failpoint in ["after_prepared", "after_old_rename", "after_swap"] {
            let root = test_root(failpoint);
            let data_dir = root.join("data");
            seed_old_data(&data_dir);

            let result = atomic_reset_data_dir_inner(&data_dir, Some(failpoint));
            assert_eq!(result, Err(ResetError::Preserved));
            assert_old_preserved(&data_dir);
            assert!(!recovery_pending(&data_dir));
            assert_eq!(fs::read_dir(&root).expect("read root").count(), 1);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn failure_after_commit_finishes_the_reset_without_mixed_state() {
        let root = test_root("after-commit");
        let data_dir = root.join("data");
        seed_old_data(&data_dir);

        atomic_reset_data_dir_inner(&data_dir, Some("after_commit"))
            .expect("committed reset is recovered as success");
        assert_fresh(&data_dir);
        assert!(!recovery_pending(&data_dir));
        assert_eq!(fs::read_dir(&root).expect("read root").count(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn startup_recovery_rolls_back_a_prepared_swap() {
        let root = test_root("prepared-recovery");
        let data_dir = root.join("data");
        seed_old_data(&data_dir);
        let parent = data_dir.parent().unwrap();
        let staging_name = format!("{STAGING_PREFIX}manual");
        let backup_name = format!("{BACKUP_PREFIX}manual");
        let staging = parent.join(&staging_name);
        let backup = parent.join(&backup_name);
        initialize_fresh_data_dir(&staging).expect("create staging");
        fs::rename(&data_dir, &backup).expect("rename old data");
        fs::rename(&staging, &data_dir).expect("swap fresh data");
        manual_journal(&data_dir, staging_name, backup_name, ResetPhase::Prepared);

        recover_interrupted_reset(&data_dir).expect("recover prepared reset");
        assert_old_preserved(&data_dir);
        assert!(!recovery_pending(&data_dir));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn startup_recovery_finishes_a_committed_swap() {
        let root = test_root("swapped-recovery");
        let data_dir = root.join("data");
        seed_old_data(&data_dir);
        let parent = data_dir.parent().unwrap();
        let staging_name = format!("{STAGING_PREFIX}manual");
        let backup_name = format!("{BACKUP_PREFIX}manual");
        let staging = parent.join(&staging_name);
        let backup = parent.join(&backup_name);
        initialize_fresh_data_dir(&staging).expect("create staging");
        fs::rename(&data_dir, &backup).expect("rename old data");
        fs::rename(&staging, &data_dir).expect("swap fresh data");
        manual_journal(&data_dir, staging_name, backup_name, ResetPhase::Swapped);

        recover_interrupted_reset(&data_dir).expect("finish committed reset");
        assert_fresh(&data_dir);
        assert!(!backup.exists());
        assert!(!recovery_pending(&data_dir));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn both_phase_markers_prefer_the_committed_swapped_state() {
        let root = test_root("both-markers");
        let data_dir = root.join("data");
        seed_old_data(&data_dir);
        let parent = data_dir.parent().unwrap();
        let staging_name = format!("{STAGING_PREFIX}manual");
        let backup_name = format!("{BACKUP_PREFIX}manual");
        let staging = parent.join(&staging_name);
        let backup = parent.join(&backup_name);
        initialize_fresh_data_dir(&staging).expect("create staging");
        fs::rename(&data_dir, &backup).expect("rename old data");
        fs::rename(&staging, &data_dir).expect("swap fresh data");
        manual_journal(
            &data_dir,
            staging_name.clone(),
            backup_name.clone(),
            ResetPhase::Prepared,
        );
        manual_journal(&data_dir, staging_name, backup_name, ResetPhase::Swapped);

        recover_interrupted_reset(&data_dir).expect("prefer committed marker");
        assert_fresh(&data_dir);
        assert!(!recovery_pending(&data_dir));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_journal_fails_closed_without_following_external_content() {
        use std::os::unix::fs::symlink;

        let root = test_root("symlink-journal");
        let data_dir = root.join("data");
        seed_old_data(&data_dir);
        let external = root.join("external-journal.json");
        fs::write(&external, b"{}").expect("write external journal");
        symlink(
            &external,
            journal_path(&data_dir, ResetPhase::Prepared).unwrap(),
        )
        .expect("create journal symlink");

        assert!(recover_interrupted_reset(&data_dir).is_err());
        assert_old_preserved(&data_dir);
        assert_eq!(
            fs::read(&external)
                .expect("external file remains")
                .as_slice(),
            b"{}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_journal_path_fails_closed_without_touching_old_data() {
        let root = test_root("invalid-journal");
        let data_dir = root.join("data");
        seed_old_data(&data_dir);
        let journal = ResetJournal {
            version: RESET_JOURNAL_VERSION,
            data_dir_name: "data".into(),
            staging_name: "../escape".into(),
            backup_name: format!("{BACKUP_PREFIX}manual"),
            phase: ResetPhase::Prepared,
        };
        write_new_journal(
            &journal_path(&data_dir, ResetPhase::Prepared).unwrap(),
            &journal,
        )
        .expect("write journal");

        assert!(recover_interrupted_reset(&data_dir).is_err());
        assert_old_preserved(&data_dir);
        let _ = fs::remove_dir_all(root);
    }
}
