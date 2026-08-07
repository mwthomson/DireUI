use std::path::{Path, PathBuf};

pub fn backup_path(path: &Path) -> PathBuf {
    let mut file_name = path.file_name().unwrap_or_default().to_os_string();
    file_name.push(".bak");
    path.with_file_name(file_name)
}

// A single rolling backup (path.bak), not a version history: each save
// overwrites whatever backup already exists with the content that was
// on disk immediately before this save, per the Backup Preference's v1
// scope (see CONTEXT.md / issue #9).
pub fn write_with_backup(path: &Path, content: &str, backup_enabled: bool) -> std::io::Result<()> {
    // Best-effort: the backup is a safety net around the save, not the
    // save itself, so a failed backup (e.g. disk full, no prior file)
    // must not block writing the user's actual content.
    if backup_enabled {
        if let Err(err) = std::fs::copy(path, backup_path(path)) {
            eprintln!(
                "warning: failed to back up {} before save: {err}",
                path.display()
            );
        }
    }
    std::fs::write(path, content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_path_appends_a_bak_extension() {
        let path = Path::new("/home/user/direwolf.conf");

        assert_eq!(
            backup_path(path),
            PathBuf::from("/home/user/direwolf.conf.bak")
        );
    }

    fn temp_config_path(test_name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("direui-test-{test_name}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("direwolf.conf")
    }

    #[test]
    fn write_with_backup_writes_the_new_content() {
        let path = temp_config_path("writes-new-content");
        std::fs::write(&path, "CHANNEL 0\n").unwrap();

        write_with_backup(&path, "CHANNEL 1\n", false).unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "CHANNEL 1\n");
    }

    #[test]
    fn write_with_backup_backs_up_the_pre_save_content_when_enabled() {
        let path = temp_config_path("backs-up-when-enabled");
        std::fs::write(&path, "CHANNEL 0\n").unwrap();

        write_with_backup(&path, "CHANNEL 1\n", true).unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "CHANNEL 1\n");
        assert_eq!(
            std::fs::read_to_string(backup_path(&path)).unwrap(),
            "CHANNEL 0\n"
        );
    }

    #[test]
    fn write_with_backup_creates_no_backup_file_when_disabled() {
        let path = temp_config_path("no-backup-when-disabled");
        std::fs::write(&path, "CHANNEL 0\n").unwrap();

        write_with_backup(&path, "CHANNEL 1\n", false).unwrap();

        assert!(!backup_path(&path).exists());
    }

    #[test]
    fn write_with_backup_overwrites_the_existing_backup_on_a_second_save() {
        let path = temp_config_path("rolling-backup");
        std::fs::write(&path, "CHANNEL 0\n").unwrap();

        write_with_backup(&path, "CHANNEL 1\n", true).unwrap();
        write_with_backup(&path, "CHANNEL 2\n", true).unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "CHANNEL 2\n");
        assert_eq!(
            std::fs::read_to_string(backup_path(&path)).unwrap(),
            "CHANNEL 1\n"
        );
    }

    #[test]
    fn write_with_backup_does_not_delete_an_existing_backup_once_disabled() {
        let path = temp_config_path("disabled-preserves-backup");
        std::fs::write(&path, "CHANNEL 0\n").unwrap();
        write_with_backup(&path, "CHANNEL 1\n", true).unwrap();

        write_with_backup(&path, "CHANNEL 2\n", false).unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "CHANNEL 2\n");
        assert_eq!(
            std::fs::read_to_string(backup_path(&path)).unwrap(),
            "CHANNEL 0\n"
        );
    }

    #[test]
    fn write_with_backup_still_writes_content_when_the_backup_copy_fails() {
        // No pre-existing file at `path`, so the backup copy step has
        // nothing to read from and fails — the content write must still
        // go through.
        let path = temp_config_path("backup-copy-fails");

        write_with_backup(&path, "CHANNEL 1\n", true).unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "CHANNEL 1\n");
    }
}
