use std::path::PathBuf;

use crate::state::AppState;

pub fn resolve_state_path(xdg_config_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let config_dir = xdg_config_home
        .map(PathBuf::from)
        .or_else(|| home.map(|h| PathBuf::from(h).join(".config")))?;
    Some(config_dir.join("direui").join("state.json"))
}

pub fn ensure_config_file_exists(path: &std::path::Path) -> std::io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::File::create(path)?;
    Ok(())
}

pub struct StateStore {
    path: PathBuf,
}

impl StateStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> AppState {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, state: &AppState) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(state)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&self.path, json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_state_path(test_name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("direui-test-{test_name}-{nanos}"))
            .join("state.json")
    }

    #[test]
    fn prefers_xdg_config_home_when_set() {
        let path = resolve_state_path(Some("/custom/config"), Some("/home/user"));

        assert_eq!(path, Some(PathBuf::from("/custom/config/direui/state.json")));
    }

    #[test]
    fn falls_back_to_home_dot_config_when_xdg_unset() {
        let path = resolve_state_path(None, Some("/home/user"));

        assert_eq!(
            path,
            Some(PathBuf::from("/home/user/.config/direui/state.json"))
        );
    }

    #[test]
    fn returns_none_when_neither_is_set() {
        let path = resolve_state_path(None, None);

        assert_eq!(path, None);
    }

    #[test]
    fn ensure_config_file_exists_creates_a_missing_file() {
        let path = temp_state_path("ensure-missing").with_file_name("aprs.conf");

        ensure_config_file_exists(&path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn ensure_config_file_exists_does_not_clobber_existing_content() {
        let path = temp_state_path("ensure-existing").with_file_name("aprs.conf");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "CHANNEL 0\n").unwrap();

        ensure_config_file_exists(&path).unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "CHANNEL 0\n");
    }

    #[test]
    fn loading_a_missing_file_returns_default_state() {
        let store = StateStore::new(temp_state_path("missing"));

        let state = store.load();

        assert_eq!(state, AppState::default());
    }

    #[test]
    fn saved_state_survives_a_reload_via_a_new_store_instance() {
        let path = temp_state_path("roundtrip");
        let mut state = AppState::default();
        state.add_known_config(PathBuf::from("/home/user/.direwolf.conf"));

        StateStore::new(path.clone()).save(&state).unwrap();
        let reloaded = StateStore::new(path).load();

        assert_eq!(reloaded, state);
    }

    #[test]
    fn loading_a_state_file_written_before_backup_preference_existed_preserves_other_fields() {
        let path = temp_state_path("pre-backup-preference-upgrade");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"known_configs":["/home/user/.direwolf.conf"],"active_config":"/home/user/.direwolf.conf"}"#,
        )
        .unwrap();

        let state = StateStore::new(path).load();

        assert_eq!(
            state.known_configs,
            vec![PathBuf::from("/home/user/.direwolf.conf")]
        );
        assert_eq!(
            state.active_config,
            Some(PathBuf::from("/home/user/.direwolf.conf"))
        );
        assert_eq!(state.backup_preference, false);
    }

    #[test]
    fn saved_state_preserves_backup_preference_across_reload() {
        let path = temp_state_path("backup-preference-roundtrip");
        let mut state = AppState::default();
        state.set_backup_preference(true);

        StateStore::new(path.clone()).save(&state).unwrap();
        let reloaded = StateStore::new(path).load();

        assert_eq!(reloaded.backup_preference, true);
    }
}
