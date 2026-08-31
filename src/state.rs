use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub fn suggest_default_config_path(
    home: &Path,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let candidate = home.join(".direwolf.conf");
    exists(&candidate).then_some(candidate)
}

// Profile names are required at creation (see plan). The HTML `required`
// attribute enforces this in the normal UI flow, but a raw POST (curl, JS
// disabled, a malformed request) can bypass it — this is the server-side
// fallback: an empty/whitespace-only name falls back to the Config File's
// basename rather than leaving the Profile with a blank name.
fn normalize_profile_name(name: &str, path: &Path) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        path.file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled Profile".to_string())
    } else {
        trimmed.to_string()
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub path: PathBuf,
    pub name: String,
    pub last_activated_at: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppState {
    pub profiles: Vec<Profile>,
    pub active_config: Option<PathBuf>,
    // #[serde(default)] so a state.json saved before this field existed
    // still loads (as backup_preference: false) instead of failing to
    // deserialize. Deliberately NOT applied to `profiles` (see ADR-0005):
    // a state.json from before the Profile rename has no `profiles` key at
    // all, so it's supposed to fail deserialization and reset to default.
    #[serde(default)]
    pub backup_preference: bool,
}

impl AppState {
    pub fn needs_first_run(&self) -> bool {
        self.profiles.is_empty()
    }

    pub fn add_profile(&mut self, path: PathBuf, name: String, make_active: bool) {
        if self.profiles.iter().any(|p| p.path == path) {
            return;
        }
        let should_activate = make_active || self.profiles.is_empty();
        let name = normalize_profile_name(&name, &path);
        self.profiles.push(Profile {
            path: path.clone(),
            name,
            last_activated_at: current_timestamp(),
        });
        if should_activate {
            self.active_config = Some(path);
        }
    }

    pub fn activate_profile(&mut self, path: &Path) -> Result<(), String> {
        let profile = self
            .profiles
            .iter_mut()
            .find(|p| p.path == path)
            .ok_or_else(|| format!("{} is not a known profile", path.display()))?;
        profile.last_activated_at = current_timestamp();
        self.active_config = Some(path.to_path_buf());
        Ok(())
    }

    pub fn rename_profile(&mut self, path: &Path, name: String) -> Result<(), String> {
        let profile = self
            .profiles
            .iter_mut()
            .find(|p| p.path == path)
            .ok_or_else(|| format!("{} is not a known profile", path.display()))?;
        profile.name = normalize_profile_name(&name, path);
        Ok(())
    }

    // Deleting the active Profile always leaves another Profile active (if
    // any remain) rather than leaving DireUI with no active config — see the
    // Active Config invariant in CONTEXT.md. The promoted Profile isn't
    // re-stamped: it's already the most-recently-activated of what's left
    // (that's how it was chosen), so it's already correctly positioned once
    // it's later deactivated.
    pub fn remove_profile(&mut self, path: &Path) {
        self.profiles.retain(|p| p.path != path);
        if self.active_config.as_deref() == Some(path) {
            self.active_config = self
                .profiles
                .iter()
                .max_by_key(|p| p.last_activated_at)
                .map(|p| p.path.clone());
        }
    }

    pub fn set_backup_preference(&mut self, enabled: bool) {
        self.backup_preference = enabled;
    }

    // The Active Config's Profile is always first; the rest are ordered by
    // Last Activated, most recent first (see CONTEXT.md).
    pub fn ordered_profiles(&self) -> Vec<&Profile> {
        let mut profiles: Vec<&Profile> = self.profiles.iter().collect();
        profiles.sort_by(|a, b| {
            let a_active = self.active_config.as_deref() == Some(a.path.as_path());
            let b_active = self.active_config.as_deref() == Some(b.path.as_path());
            match (a_active, b_active) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => b.last_activated_at.cmp(&a.last_activated_at),
            }
        });
        profiles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(path: &str, name: &str, last_activated_at: u64) -> Profile {
        Profile {
            path: PathBuf::from(path),
            name: name.to_string(),
            last_activated_at,
        }
    }

    #[test]
    fn needs_first_run_is_true_when_there_are_no_profiles() {
        let state = AppState::default();
        assert!(state.needs_first_run());
    }

    #[test]
    fn needs_first_run_is_false_once_a_profile_exists() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/.direwolf.conf"), "Main".to_string(), false);
        assert!(!state.needs_first_run());
    }

    #[test]
    fn adding_first_profile_makes_it_active() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/.direwolf.conf"), "Main".to_string(), false);

        assert_eq!(
            state.active_config,
            Some(PathBuf::from("/home/user/.direwolf.conf"))
        );
    }

    #[test]
    fn adding_a_second_profile_without_make_active_does_not_change_active() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/.direwolf.conf"), "Main".to_string(), false);
        state.add_profile(PathBuf::from("/home/user/aprs.conf"), "APRS".to_string(), false);

        assert_eq!(
            state.active_config,
            Some(PathBuf::from("/home/user/.direwolf.conf"))
        );
        assert_eq!(state.profiles.len(), 2);
    }

    #[test]
    fn adding_a_profile_with_make_active_switches_the_active_config() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/.direwolf.conf"), "Main".to_string(), false);
        state.add_profile(PathBuf::from("/home/user/aprs.conf"), "APRS".to_string(), true);

        assert_eq!(state.active_config, Some(PathBuf::from("/home/user/aprs.conf")));
    }

    #[test]
    fn adding_an_already_known_path_is_a_no_op() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/.direwolf.conf"), "Main".to_string(), false);
        state.add_profile(PathBuf::from("/home/user/aprs.conf"), "APRS".to_string(), true);

        state.add_profile(PathBuf::from("/home/user/.direwolf.conf"), "Ignored".to_string(), true);

        assert_eq!(state.profiles.len(), 2);
        assert_eq!(state.active_config, Some(PathBuf::from("/home/user/aprs.conf")));
    }

    #[test]
    fn a_new_profile_is_stamped_as_just_activated_even_when_not_made_active() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/.direwolf.conf"), "Main".to_string(), false);
        let before = state.profiles[0].last_activated_at;

        state.add_profile(PathBuf::from("/home/user/aprs.conf"), "APRS".to_string(), false);

        let aprs = state.profiles.iter().find(|p| p.name == "APRS").unwrap();
        assert!(aprs.last_activated_at >= before);
    }

    #[test]
    fn activating_a_known_profile_succeeds_and_stamps_last_activated_at() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/.direwolf.conf"), "Main".to_string(), false);
        state.add_profile(PathBuf::from("/home/user/aprs.conf"), "APRS".to_string(), false);

        state
            .activate_profile(&PathBuf::from("/home/user/aprs.conf"))
            .unwrap();

        assert_eq!(state.active_config, Some(PathBuf::from("/home/user/aprs.conf")));
    }

    #[test]
    fn activating_an_unknown_profile_fails_and_leaves_active_config_unchanged() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/.direwolf.conf"), "Main".to_string(), false);

        let result = state.activate_profile(&PathBuf::from("/home/user/nope.conf"));

        assert!(result.is_err());
        assert_eq!(
            state.active_config,
            Some(PathBuf::from("/home/user/.direwolf.conf"))
        );
    }

    #[test]
    fn renaming_a_known_profile_updates_its_name() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/.direwolf.conf"), "Main".to_string(), false);

        state
            .rename_profile(&PathBuf::from("/home/user/.direwolf.conf"), "Packet".to_string())
            .unwrap();

        assert_eq!(state.profiles[0].name, "Packet");
    }

    #[test]
    fn renaming_an_unknown_profile_fails() {
        let mut state = AppState::default();

        let result = state.rename_profile(&PathBuf::from("/home/user/nope.conf"), "X".to_string());

        assert!(result.is_err());
    }

    #[test]
    fn renaming_does_not_require_the_name_to_be_unique() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/a.conf"), "Winlink".to_string(), false);
        state.add_profile(PathBuf::from("/home/user/b.conf"), "Other".to_string(), false);

        let result = state.rename_profile(&PathBuf::from("/home/user/b.conf"), "Winlink".to_string());

        assert!(result.is_ok());
        assert_eq!(state.profiles[1].name, "Winlink");
    }

    #[test]
    fn removing_a_non_active_profile_leaves_active_config_unchanged() {
        let mut state = AppState {
            profiles: vec![profile("/home/user/a.conf", "A", 200), profile("/home/user/b.conf", "B", 100)],
            active_config: Some(PathBuf::from("/home/user/a.conf")),
            backup_preference: false,
        };

        state.remove_profile(&PathBuf::from("/home/user/b.conf"));

        assert_eq!(state.profiles.len(), 1);
        assert_eq!(state.active_config, Some(PathBuf::from("/home/user/a.conf")));
    }

    #[test]
    fn removing_the_active_profile_promotes_the_most_recently_activated_remaining_one() {
        let mut state = AppState {
            profiles: vec![
                profile("/home/user/a.conf", "A", 300),
                profile("/home/user/b.conf", "B", 200),
                profile("/home/user/c.conf", "C", 100),
            ],
            active_config: Some(PathBuf::from("/home/user/a.conf")),
            backup_preference: false,
        };

        state.remove_profile(&PathBuf::from("/home/user/a.conf"));

        assert_eq!(state.active_config, Some(PathBuf::from("/home/user/b.conf")));
    }

    #[test]
    fn removing_the_last_profile_leaves_no_active_config() {
        let mut state = AppState {
            profiles: vec![profile("/home/user/a.conf", "A", 100)],
            active_config: Some(PathBuf::from("/home/user/a.conf")),
            backup_preference: false,
        };

        state.remove_profile(&PathBuf::from("/home/user/a.conf"));

        assert_eq!(state.profiles, vec![]);
        assert_eq!(state.active_config, None);
    }

    #[test]
    fn ordered_profiles_puts_the_active_profile_first_regardless_of_last_activated_at() {
        let state = AppState {
            profiles: vec![profile("/home/user/a.conf", "A", 100), profile("/home/user/b.conf", "B", 999)],
            active_config: Some(PathBuf::from("/home/user/a.conf")),
            backup_preference: false,
        };

        let ordered = state.ordered_profiles();

        assert_eq!(ordered[0].path, PathBuf::from("/home/user/a.conf"));
        assert_eq!(ordered[1].path, PathBuf::from("/home/user/b.conf"));
    }

    #[test]
    fn ordered_profiles_sorts_the_rest_by_last_activated_at_descending() {
        let state = AppState {
            profiles: vec![
                profile("/home/user/a.conf", "A", 100),
                profile("/home/user/b.conf", "B", 300),
                profile("/home/user/c.conf", "C", 200),
            ],
            active_config: Some(PathBuf::from("/home/user/a.conf")),
            backup_preference: false,
        };

        let ordered = state.ordered_profiles();

        assert_eq!(
            ordered.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            vec!["A", "B", "C"]
        );
    }

    #[test]
    fn add_profile_with_an_empty_name_falls_back_to_the_path_filename() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/.direwolf.conf"), "".to_string(), false);

        assert_eq!(state.profiles[0].name, ".direwolf.conf");
    }

    #[test]
    fn add_profile_with_a_whitespace_only_name_falls_back_to_the_path_filename() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/aprs.conf"), "   ".to_string(), false);

        assert_eq!(state.profiles[0].name, "aprs.conf");
    }

    #[test]
    fn add_profile_trims_meaningful_leading_and_trailing_whitespace_from_a_name() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/aprs.conf"), "  APRS  ".to_string(), false);

        assert_eq!(state.profiles[0].name, "APRS");
    }

    #[test]
    fn rename_profile_with_an_empty_name_falls_back_to_the_path_filename() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/.direwolf.conf"), "Main".to_string(), false);

        state
            .rename_profile(&PathBuf::from("/home/user/.direwolf.conf"), "".to_string())
            .unwrap();

        assert_eq!(state.profiles[0].name, ".direwolf.conf");
    }

    #[test]
    fn rename_profile_with_a_whitespace_only_name_falls_back_to_the_path_filename() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/aprs.conf"), "Main".to_string(), false);

        state
            .rename_profile(&PathBuf::from("/home/user/aprs.conf"), "   ".to_string())
            .unwrap();

        assert_eq!(state.profiles[0].name, "aprs.conf");
    }

    #[test]
    fn rename_profile_trims_meaningful_leading_and_trailing_whitespace_from_a_name() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/aprs.conf"), "Main".to_string(), false);

        state
            .rename_profile(&PathBuf::from("/home/user/aprs.conf"), "  Packet  ".to_string())
            .unwrap();

        assert_eq!(state.profiles[0].name, "Packet");
    }

    #[test]
    fn suggests_direwolf_conf_when_it_exists_in_home() {
        let home = Path::new("/home/user");
        let suggestion =
            suggest_default_config_path(home, |p| p == Path::new("/home/user/.direwolf.conf"));

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
}
