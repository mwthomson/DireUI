use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub fn suggest_default_config_path(
    home: &Path,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let candidate = home.join(".direwolf.conf");
    exists(&candidate).then_some(candidate)
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppState {
    pub known_configs: Vec<PathBuf>,
    pub active_config: Option<PathBuf>,
    // #[serde(default)] so a state.json saved before this field existed
    // still loads (as backup_preference: false) instead of failing to
    // deserialize and silently falling back to AppState::default(),
    // which would wipe known_configs/active_config too.
    #[serde(default)]
    pub backup_preference: bool,
}

impl AppState {
    pub fn needs_first_run(&self) -> bool {
        self.known_configs.is_empty()
    }

    pub fn add_known_config(&mut self, path: PathBuf) {
        if self.known_configs.contains(&path) {
            return;
        }
        if self.active_config.is_none() {
            self.active_config = Some(path.clone());
        }
        self.known_configs.push(path);
    }

    pub fn set_active_config(&mut self, path: &PathBuf) -> Result<(), String> {
        if !self.known_configs.contains(path) {
            return Err(format!("{} is not a known config", path.display()));
        }
        self.active_config = Some(path.clone());
        Ok(())
    }

    pub fn set_backup_preference(&mut self, enabled: bool) {
        self.backup_preference = enabled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adding_first_config_makes_it_active() {
        let mut state = AppState::default();
        state.add_known_config(PathBuf::from("/home/user/.direwolf.conf"));

        assert_eq!(
            state.active_config,
            Some(PathBuf::from("/home/user/.direwolf.conf"))
        );
    }

    #[test]
    fn adding_second_config_does_not_change_active() {
        let mut state = AppState::default();
        state.add_known_config(PathBuf::from("/home/user/.direwolf.conf"));
        state.add_known_config(PathBuf::from("/home/user/aprs.conf"));

        assert_eq!(
            state.active_config,
            Some(PathBuf::from("/home/user/.direwolf.conf"))
        );
        assert_eq!(
            state.known_configs,
            vec![
                PathBuf::from("/home/user/.direwolf.conf"),
                PathBuf::from("/home/user/aprs.conf"),
            ]
        );
    }

    #[test]
    fn switching_active_to_a_known_config_succeeds() {
        let mut state = AppState::default();
        state.add_known_config(PathBuf::from("/home/user/.direwolf.conf"));
        state.add_known_config(PathBuf::from("/home/user/aprs.conf"));

        state
            .set_active_config(&PathBuf::from("/home/user/aprs.conf"))
            .unwrap();

        assert_eq!(
            state.active_config,
            Some(PathBuf::from("/home/user/aprs.conf"))
        );
    }

    #[test]
    fn adding_an_already_known_path_is_a_no_op() {
        let mut state = AppState::default();
        state.add_known_config(PathBuf::from("/home/user/.direwolf.conf"));
        state.add_known_config(PathBuf::from("/home/user/aprs.conf"));
        state.set_active_config(&PathBuf::from("/home/user/aprs.conf")).unwrap();

        state.add_known_config(PathBuf::from("/home/user/.direwolf.conf"));

        assert_eq!(
            state.known_configs,
            vec![
                PathBuf::from("/home/user/.direwolf.conf"),
                PathBuf::from("/home/user/aprs.conf"),
            ]
        );
        assert_eq!(
            state.active_config,
            Some(PathBuf::from("/home/user/aprs.conf"))
        );
    }

    #[test]
    fn suggests_direwolf_conf_when_it_exists_in_home() {
        let home = Path::new("/home/user");
        let suggestion = suggest_default_config_path(home, |p| {
            p == Path::new("/home/user/.direwolf.conf")
        });

        assert_eq!(suggestion, Some(PathBuf::from("/home/user/.direwolf.conf")));
    }

    #[test]
    fn suggests_nothing_when_no_conventional_file_exists() {
        let home = Path::new("/home/user");
        let suggestion = suggest_default_config_path(home, |_| false);

        assert_eq!(suggestion, None);
    }

    #[test]
    fn backup_preference_defaults_to_disabled() {
        let state = AppState::default();

        assert_eq!(state.backup_preference, false);
    }

    #[test]
    fn set_backup_preference_updates_the_flag() {
        let mut state = AppState::default();

        state.set_backup_preference(true);
        assert_eq!(state.backup_preference, true);

        state.set_backup_preference(false);
        assert_eq!(state.backup_preference, false);
    }

    #[test]
    fn switching_active_to_an_unknown_config_fails() {
        let mut state = AppState::default();
        state.add_known_config(PathBuf::from("/home/user/.direwolf.conf"));

        let result = state.set_active_config(&PathBuf::from("/home/user/nope.conf"));

        assert!(result.is_err());
        assert_eq!(
            state.active_config,
            Some(PathBuf::from("/home/user/.direwolf.conf"))
        );
    }
}
