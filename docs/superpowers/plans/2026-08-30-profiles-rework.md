# Profiles Rework Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename DireUI's "Configurations" page to "Profiles" throughout the codebase, give every Profile a user-editable name, always list the active Profile first, let the user rename and delete Profiles (with a choice to also delete the underlying Config File), and give the Edit directives / Edit raw config pages save feedback and a Cancel button.

**Architecture:** DireUI is a Rust/axum server that renders plain HTML strings (no templating engine, no client-side router) with a handful of HTMX-driven partial swaps. This plan follows that shape exactly: new behavior is new route handlers + new `format!`-based view functions + a small amount of vanilla JS for the one native `<dialog>` interaction and the rename-field toggle. No new dependency is added — a Profile's "Last Activated" time is a `u64` Unix-epoch-seconds value from `std::time::SystemTime`, and the one-time save/delete feedback banner is carried across a redirect via a hand-rolled `flash` query parameter (the app has no session/cookie layer).

**Tech Stack:** Rust (edition 2024), axum 0.8, serde/serde_json, tokio. Client side: server-rendered HTML, htmx (already vendored), and one new small vanilla-JS file (`assets/app.js`) — no new npm/cargo dependency.

## Global Constraints

- No new Cargo dependency. Timestamps are `u64` Unix-epoch-seconds via `std::time::SystemTime`; there is no `chrono`/`time` crate in this project and none should be added for this.
- Follow `CONTEXT.md`'s decided vocabulary exactly: **Profile** (not "Known Config"/"Configuration"), **Active Config**, **Last Activated**, **Remove** (DireUI record only) vs **Delete** (also deletes the Config File from disk). Use these words in UI copy, route names, and code identifiers.
- `state.json`'s schema change is intentionally breaking, per `docs/adr/0005-no-state-json-migration.md` — do not write migration code. An old-shape `state.json` should fail to deserialize and fall back to `AppState::default()` (empty Profile list).
- Every Save/Cancel/Remove/Delete outcome must use this exact user-facing wording where specified: `changes saved`, `no changes to save`, `save failed: [reason]`, `Profile removed`, `Profile and file deleted`, `couldn't delete file: [reason]`.
- Cancel on the Edit directives / Edit raw config pages always discards silently and returns to `/` — no confirmation prompt, even with unsaved changes.
- Deleting the active Profile is allowed (with an on-screen warning in the delete dialog); DireUI must never end up with Profiles present but none active — another Profile is auto-promoted immediately.
- Profile names are required at creation time but are **not** required to be unique.
- Every task must leave `cargo build` and `cargo test` passing — this is a single small binary crate with no workspace, so tasks are sized around what keeps the whole crate compiling, not around artificial per-file splits.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/state.rs` | `Profile` struct + `AppState` (Profiles, Active Config, ordering, Remove/Delete's state-side effect). No I/O. |
| `src/store.rs` | Loading/saving `state.json`. Unchanged shape of responsibility; tests updated for the new `Profile`-shaped schema and ADR-0005's reset-on-incompatible-shape behavior. |
| `src/flash.rs` *(new)* | The one-time `?flash=` query-param message shown on the Profiles page after a redirect. Pure string/enum logic, no I/O, no `AppState` dependency. |
| `src/main.rs` | Routes and handlers: Profile CRUD, Save flows for the two editors, flash wiring. |
| `src/views.rs` | All HTML rendering: Profiles list, Add-profile form, delete dialog markup, the two editor pages (with Cancel + save-error rendering), flash banner. |
| `assets/app.js` *(new)* | The two bits of vanilla JS this design needs: toggling a Profile row into rename-edit mode, and driving the shared delete `<dialog>` (open it with the right row's data, two-step confirm for actually deleting the file). |
| `assets/style.css` | Renamed `.config-list`/`.config-badge` → `.profile-list`/`.profile-badge`; new rules for the name/rename UI, icon buttons, the delete dialog, the flash banner, and a plain-link "button" style for Cancel. |

---

### Task 1: Open the tracking issue

**Files:** none (GitHub only)

- [ ] **Step 1: Create the issue, cross-linked to #13**

```bash
gh issue create --repo mwthomson/DireUI --title "Rename Configurations to Profiles: naming, ordering, deletion, save/cancel UX" --body "$(cat <<'EOF'
Splits a focused slice off #13's v2 PRD: the Profile rename plus naming, ordering,
deletion, and Save/Cancel UX. The bigger v2 features #13 also scopes (process
control, live status, dry-run validation, expanded Curated Directive coverage)
stay out of scope here and remain tracked on #13.

## Scope

- Full rename: "Configurations" → "Profiles" in UI text, routes, component/file
  names, and variables.
- Every Profile gets a required, user-editable name (path stays the real
  identifier; names don't need to be unique).
- The active Profile is always the first row; the rest are ordered by Last
  Activated, most recent first.
- Delete a Profile via a trashcan icon: choose Remove (DireUI record only) or
  Delete (also deletes the Config File from disk, extra-confirmed).
- Save on Edit directives / Edit raw config shows feedback (changes saved / no
  changes to save / save failed: [reason]) and returns to Profiles on success
  or no-op.
- Cancel button on both editor pages; always discards silently.

## Out of scope (stays on #13)

Process control, live status, dry-run validation, expanded Curated Directive
coverage, Backup History version management.

See CONTEXT.md for the agreed vocabulary (Profile, Active Config, Last
Activated, Remove, Delete) and docs/adr/0005-no-state-json-migration.md for
the state.json breaking-change decision this work depends on.

Part of #13
EOF
)"
```

- [ ] **Step 2: Note the new issue number**

Record the issue number `gh issue create` prints — reference it in commit messages for the tasks below as `Refs #<n>`.

---

### Task 2: Flash message module

Standalone, pure logic — no other file references it yet (`main.rs` declares the module but doesn't call into it until Task 5).

**Files:**
- Create: `src/flash.rs`
- Modify: `src/main.rs:1-6` (add `mod flash;`)

**Interfaces:**
- Produces (used by Tasks 5, 6, 8): `flash::Flash` enum with variants `Saved`, `NoChange`, `Removed`, `Deleted`, `DeleteFailed(String)`; `Flash::to_query_string(&self) -> String`; `Flash::from_params(params: &HashMap<String, String>) -> Option<Flash>`; `Flash::message(&self) -> String`.

- [ ] **Step 1: Write the failing tests and implementation together**

Create `src/flash.rs`:

```rust
use std::collections::HashMap;

// A one-time message shown on the Profiles page after a redirect, carried
// via a `flash` query parameter since the app has no session/cookie layer.
#[derive(Debug, Clone, PartialEq)]
pub enum Flash {
    Saved,
    NoChange,
    Removed,
    Deleted,
    DeleteFailed(String),
}

impl Flash {
    pub fn to_query_string(&self) -> String {
        match self {
            Flash::Saved => "flash=saved".to_string(),
            Flash::NoChange => "flash=nochange".to_string(),
            Flash::Removed => "flash=removed".to_string(),
            Flash::Deleted => "flash=deleted".to_string(),
            Flash::DeleteFailed(reason) => {
                format!("flash=delete_failed&reason={}", percent_encode(reason))
            }
        }
    }

    pub fn from_params(params: &HashMap<String, String>) -> Option<Flash> {
        match params.get("flash")?.as_str() {
            "saved" => Some(Flash::Saved),
            "nochange" => Some(Flash::NoChange),
            "removed" => Some(Flash::Removed),
            "deleted" => Some(Flash::Deleted),
            "delete_failed" => Some(Flash::DeleteFailed(
                params.get("reason").cloned().unwrap_or_default(),
            )),
            _ => None,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Flash::Saved => "changes saved".to_string(),
            Flash::NoChange => "no changes to save".to_string(),
            Flash::Removed => "Profile removed".to_string(),
            Flash::Deleted => "Profile and file deleted".to_string(),
            Flash::DeleteFailed(reason) => format!("couldn't delete file: {reason}"),
        }
    }
}

// axum's Query extractor percent-decodes incoming values for us, so this
// only needs to handle the outgoing direction (building a Redirect's URI).
fn percent_encode(input: &str) -> String {
    input
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn saved_round_trips_through_query_string() {
        assert_eq!(Flash::Saved.to_query_string(), "flash=saved");
        assert_eq!(Flash::from_params(&params(&[("flash", "saved")])), Some(Flash::Saved));
    }

    #[test]
    fn nochange_round_trips_through_query_string() {
        assert_eq!(Flash::NoChange.to_query_string(), "flash=nochange");
        assert_eq!(
            Flash::from_params(&params(&[("flash", "nochange")])),
            Some(Flash::NoChange)
        );
    }

    #[test]
    fn removed_round_trips_through_query_string() {
        assert_eq!(Flash::Removed.to_query_string(), "flash=removed");
        assert_eq!(Flash::from_params(&params(&[("flash", "removed")])), Some(Flash::Removed));
    }

    #[test]
    fn deleted_round_trips_through_query_string() {
        assert_eq!(Flash::Deleted.to_query_string(), "flash=deleted");
        assert_eq!(Flash::from_params(&params(&[("flash", "deleted")])), Some(Flash::Deleted));
    }

    #[test]
    fn delete_failed_percent_encodes_the_reason_in_the_query_string() {
        let flash = Flash::DeleteFailed("Permission denied (os error 13)".to_string());

        assert_eq!(
            flash.to_query_string(),
            "flash=delete_failed&reason=Permission%20denied%20%28os%20error%2013%29"
        );
    }

    #[test]
    fn delete_failed_round_trips_the_reason_from_params() {
        let params = params(&[("flash", "delete_failed"), ("reason", "disk full")]);

        assert_eq!(
            Flash::from_params(&params),
            Some(Flash::DeleteFailed("disk full".to_string()))
        );
    }

    #[test]
    fn from_params_returns_none_when_flash_is_absent() {
        assert_eq!(Flash::from_params(&HashMap::new()), None);
    }

    #[test]
    fn from_params_returns_none_for_an_unrecognized_flash_value() {
        assert_eq!(Flash::from_params(&params(&[("flash", "bogus")])), None);
    }

    #[test]
    fn message_text_matches_the_agreed_wording() {
        assert_eq!(Flash::Saved.message(), "changes saved");
        assert_eq!(Flash::NoChange.message(), "no changes to save");
        assert_eq!(Flash::Removed.message(), "Profile removed");
        assert_eq!(Flash::Deleted.message(), "Profile and file deleted");
        assert_eq!(
            Flash::DeleteFailed("disk full".to_string()).message(),
            "couldn't delete file: disk full"
        );
    }
}
```

- [ ] **Step 2: Wire the module into the crate**

Modify `src/main.rs:1-6`:

```rust
mod backup;
mod bind_config;
mod config;
mod flash;
mod state;
mod store;
mod views;
```

- [ ] **Step 3: Run the flash.rs tests**

Run: `cargo test --lib flash::`
Expected: all `flash::tests::*` pass.

- [ ] **Step 4: Run the full suite**

Run: `cargo build && cargo test`
Expected: builds and passes (the module is compiled now but unused outside its own tests — no warnings, since every new item is `pub`).

- [ ] **Step 5: Commit**

```bash
git add src/flash.rs src/main.rs
git commit -m "$(cat <<'EOF'
Add the one-time flash-message module for post-redirect feedback

Carries changes-saved / no-changes / removed / deleted / delete-failed
messages across a redirect to the Profiles page via a `flash` query
parameter, since the app has no session or cookie layer. Not wired into any
handler yet.
EOF
)"
```

---

### Task 3: Profile domain model + name-aware Profiles list

This is the load-bearing task: it renames `AppState.known_configs: Vec<PathBuf>` to `AppState.profiles: Vec<Profile>`, adds every new `AppState` method this feature needs, and updates every call site so the crate keeps compiling. Rename, deletion, flash messaging, and Cancel buttons are **not** wired up yet — this task's Profiles page can only add and activate a Profile, same as today, but now with a name and active-always-first ordering.

**Files:**
- Modify: `src/state.rs` (full rewrite of the struct/methods/tests)
- Modify: `src/main.rs:76-96, 376-390` (route/handler renames, `AddProfileForm`)
- Modify: `src/views.rs` (`config_manager` → `profiles_page`, `add_config_form` → `add_profile_form`, `page()` nav text, tests)
- Modify: `assets/style.css` (`.config-list`/`.config-badge` → `.profile-list`/`.profile-badge`, new `.profile-name`/`.checkbox-label` rules)

**Interfaces:**
- Produces (used by later tasks): `state::Profile { path: PathBuf, name: String, last_activated_at: u64 }`; `AppState::profiles: Vec<Profile>`; `AppState::add_profile(&mut self, path: PathBuf, name: String, make_active: bool)`; `AppState::activate_profile(&mut self, path: &Path) -> Result<(), String>`; `AppState::rename_profile(&mut self, path: &Path, name: String) -> Result<(), String>`; `AppState::remove_profile(&mut self, path: &Path)`; `AppState::ordered_profiles(&self) -> Vec<&Profile>`; `views::profiles_page(state: &AppState, flash: Option<&flash::Flash>) -> String` (the `flash` param is accepted starting now, even though nothing passes `Some` until Task 5 — `index()` is updated in Task 5 to read the query param).

- [ ] **Step 1: Write the failing `state.rs` tests for the new model**

Replace the entire contents of `src/state.rs` with:

```rust
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub fn suggest_default_config_path(
    home: &Path,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let candidate = home.join(".direwolf.conf");
    exists(&candidate).then_some(candidate)
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
        profile.name = name;
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
```

- [ ] **Step 2: Run the state.rs tests and confirm they pass in isolation**

Run: `cargo test --lib state::`
Expected: every `state::tests::*` test passes. (`cargo build`/`cargo test` for the whole crate will still fail until Step 3 — `main.rs`/`views.rs` still reference the old `known_configs`/`add_known_config`/`set_active_config` names.)

- [ ] **Step 3: Update `main.rs`'s Profile-adding/activating call sites**

Modify `src/main.rs:71-96`, replacing the `PathForm`/`add_config`/`set_active_config` block:

```rust
#[derive(Deserialize)]
struct PathForm {
    path: String,
}

#[derive(Deserialize)]
struct AddProfileForm {
    path: String,
    name: String,
    #[serde(default)]
    make_active: bool,
}

async fn add_profile(State(ctx): State<AppContext>, Form(form): Form<AddProfileForm>) -> Redirect {
    let path = PathBuf::from(form.path);
    if let Err(err) = store::ensure_config_file_exists(&path) {
        eprintln!("error: failed to create {}: {err}", path.display());
        return Redirect::to("/");
    }

    ctx.mutate_and_save(|state| state.add_profile(path, form.name, form.make_active));
    Redirect::to("/")
}

async fn activate_profile(State(ctx): State<AppContext>, Form(form): Form<PathForm>) -> Redirect {
    let path = PathBuf::from(form.path);

    ctx.mutate_and_save(|state| {
        if let Err(err) = state.activate_profile(&path) {
            eprintln!("error: {err}");
        }
    });
    Redirect::to("/")
}
```

Modify `src/main.rs:45-57` (`index`) — only the view function name changes for now (it still doesn't take the `flash` argument yet, so pass `None`; Task 5 wires the real value through):

```rust
async fn index(State(ctx): State<AppContext>) -> Html<String> {
    let state = ctx.state.lock().unwrap();
    let body = if state.needs_first_run() {
        let suggestion = ctx
            .home
            .as_deref()
            .and_then(|home| state::suggest_default_config_path(home, |p| p.exists()));
        views::first_run(suggestion.as_deref())
    } else {
        views::profiles_page(&state, None)
    };
    Html(views::page(&body, state.active_config.as_deref()))
}
```

Modify `src/main.rs:376-390` (the router), replacing the `/configs` routes:

```rust
    let app = Router::new()
        .route("/", get(index))
        .route("/profiles", axum::routing::post(add_profile))
        .route("/profiles/activate", axum::routing::post(activate_profile))
        .route(
            "/backup-preference",
            axum::routing::post(set_backup_preference),
        )
        .route("/raw", get(edit_raw_config).post(save_raw_config))
        .route("/directives", get(edit_directives).post(save_directives))
        .route("/directives/clear", axum::routing::post(clear_directive))
        .route("/status", get(status))
        .route("/vendor/htmx/htmx.min.js", get(htmx_js))
        .route("/style.css", get(style_css))
        .with_state(ctx);
```

- [ ] **Step 4: Update `views.rs`'s Profiles list rendering**

Modify `src/views.rs:48-58` (`add_config_form` → `add_profile_form`, now with a Name field and an optional "make active" checkbox):

```rust
fn add_profile_form(
    path_value: &str,
    path_placeholder: &str,
    name_placeholder: &str,
    submit_label: &str,
    show_make_active: bool,
) -> String {
    let make_active_html = if show_make_active {
        r#"<label class="checkbox-label"><input type="checkbox" name="make_active" value="true"> Make this the active profile</label>"#
    } else {
        ""
    };
    format!(
        r#"<form class="form-inline" method="post" action="/profiles">
<input type="text" name="name" value="" placeholder="{name_placeholder}" required>
<input type="text" name="path" value="{path_value}" placeholder="{path_placeholder}">
{make_active}
<button type="submit">{label}</button>
</form>"#,
        name_placeholder = html_escape(name_placeholder),
        path_value = html_escape(path_value),
        path_placeholder = html_escape(path_placeholder),
        make_active = make_active_html,
        label = html_escape(submit_label)
    )
}
```

Modify `src/views.rs:60-70` (`first_run`):

```rust
pub fn first_run(suggested_path: Option<&Path>) -> String {
    let suggestion = suggested_path
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    format!(
        r#"<h1>Welcome to DireUI</h1>
<p>Choose the Direwolf config file DireUI should manage.</p>
{}"#,
        add_profile_form(&suggestion, "/home/user/.direwolf.conf", "e.g. APRS", "Use this config", false)
    )
}
```

Replace `src/views.rs:166-210` (`config_manager` and its list-item rendering) with:

```rust
fn profile_row_html(profile: &state::Profile, is_active: bool, path_html: &str) -> String {
    let name = html_escape(&profile.name);
    let path_attr = html_escape(&profile.path.display().to_string());
    let status_html = if is_active {
        r#"<span class="pill profile-badge">active</span>"#.to_string()
    } else {
        format!(
            r#"<form method="post" action="/profiles/activate"><input type="hidden" name="path" value="{path}"><button type="submit">Switch to this</button></form>"#,
            path = path_attr
        )
    };
    format!(
        r#"<li><span class="profile-name">{name}</span><span class="config-path" title="{path_attr}">{path_html}</span>{status}</li>"#,
        name = name,
        path_attr = path_attr,
        path_html = path_html,
        status = status_html
    )
}

pub fn profiles_page(state: &AppState, flash: Option<&crate::flash::Flash>) -> String {
    let ordered = state.ordered_profiles();
    let displays: Vec<String> = ordered.iter().map(|p| p.path.display().to_string()).collect();
    let highlighted = highlight_differing_segments(&displays);

    let list_items: String = ordered
        .iter()
        .zip(highlighted.iter())
        .map(|(profile, path_html)| {
            let is_active = state.active_config.as_deref() == Some(profile.path.as_path());
            profile_row_html(profile, is_active, path_html)
        })
        .collect();

    let flash_html = flash
        .map(|f| format!(r#"<p class="flash" role="status">{}</p>"#, html_escape(&f.message())))
        .unwrap_or_default();

    format!(
        r##"<h1>Profiles</h1>
{flash}
<ul class="profile-list">{items}</ul>
{add_form}
{backup_toggle}
<nav class="actions">
<a href="/directives">Edit directives</a>
<a href="/raw">Edit raw config</a>
<div class="status-check">
<button hx-get="/status" hx-target="#server-status" hx-swap="innerHTML">Check server status</button>
<span id="server-status" aria-live="polite"></span>
</div>
</nav>"##,
        flash = flash_html,
        items = list_items,
        add_form = add_profile_form("", "/home/user/aprs.conf", "e.g. APRS", "Add profile", !state.profiles.is_empty()),
        backup_toggle = backup_preference_toggle(state.backup_preference),
    )
}
```

Modify `src/views.rs:1-3` to bring `state::Profile` into scope for the new function signature:

```rust
use std::path::Path;

use crate::state::{self, AppState};
```

Modify `src/views.rs:30-36` (`page()`'s nav link text):

```rust
<nav class="site-nav">
<span class="site-active-config">Active config: <span class="config-path">{active}</span></span>
<a href="/">Profiles</a>
</nav>
```

- [ ] **Step 5: Update `views.rs`'s existing tests to the new names/shape**

Modify `src/views.rs` tests:
- `page_always_links_back_to_the_config_manager` (was asserting `<a href="/">Configs</a>`) → rename to `page_always_links_back_to_the_profiles_page` and assert `<a href="/">Profiles</a>`.
- `config_manager_status_button_targets_a_separate_indicator_not_itself` → rename to `profiles_page_status_button_targets_a_separate_indicator_not_itself`, replace `AppState::default()` + `config_manager(&state)` with `AppState::default()` + `profiles_page(&state, None)`.
- `config_manager_marks_the_differing_segment_between_two_known_configs`, `config_manager_still_marks_the_active_config_when_paths_are_similar`, `config_manager_shows_the_full_path_as_a_title_attribute`: rename each to the `profiles_page_*` equivalent, replace the `AppState { known_configs: vec![...], ... }` literals with `AppState { profiles: vec![Profile { path: ..., name: "Test".to_string(), last_activated_at: 0 }, ...], ... }`, and replace `config_manager(&state)` calls with `profiles_page(&state, None)`.

For example, the differing-segment test becomes:

```rust
#[test]
fn profiles_page_marks_the_differing_segment_between_two_profiles() {
    let state = AppState {
        profiles: vec![
            Profile {
                path: PathBuf::from("/home/pi/aprs-config/direwolf.conf"),
                name: "APRS".to_string(),
                last_activated_at: 0,
            },
            Profile {
                path: PathBuf::from("/home/pi/packet-config/direwolf.conf"),
                name: "Packet".to_string(),
                last_activated_at: 0,
            },
        ],
        active_config: Some(PathBuf::from("/home/pi/aprs-config/direwolf.conf")),
        backup_preference: false,
    };

    let html = profiles_page(&state, None);

    assert!(html.contains(r#"<span class="config-path-diff">aprs-config</span>"#));
    assert!(html.contains(r#"<span class="config-path-diff">packet-config</span>"#));
}
```

Apply the same field-literal update to the other two renamed tests, and add `use crate::state::Profile;` (or reference it as `state::Profile` matching the module import from Step 4) at the top of the `tests` module.

- [ ] **Step 6: Rename the CSS classes and add the name/checkbox styles**

Modify `assets/style.css:119-126` (`.config-list` → `.profile-list`):

```css
.profile-list {
  list-style: none;
  margin: 0 0 1.5rem;
  padding: 0;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius);
}

.profile-list li {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding: 0.75rem 1rem;
  border-bottom: 1px solid var(--line);
}

.profile-list li:last-child {
  border-bottom: none;
}
```

Modify `assets/style.css:165-168` (`.config-badge` → `.profile-badge`):

```css
.profile-badge {
  color: var(--signal);
  border: 1px solid var(--signal);
}
```

Add, after the `.config-path-diff` rule (`assets/style.css:148-153`):

```css
.profile-name {
  font-weight: 600;
}

.checkbox-label {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  font-size: 0.85rem;
  color: var(--ink-dim);
}
```

- [ ] **Step 7: Build and run the full test suite**

Run: `cargo build && cargo test`
Expected: builds cleanly, all tests pass (some renamed, none deleted in count vs. before this task other than the ones explicitly folded into new names above).

- [ ] **Step 8: Commit**

```bash
git add src/state.rs src/main.rs src/views.rs assets/style.css
git commit -m "$(cat <<'EOF'
Rename Known Config to Profile: named, ordered Profiles list

Profiles now have a required display name and a Last Activated time. The
Profiles list always shows the active Profile first, then the rest ordered
by Last Activated (most recent first). Adding a Profile can optionally make
it active immediately via a new checkbox on the Add form.

Refs #<issue-number-from-task-1>
EOF
)"
```

---

### Task 4: state.json regression coverage for ADR-0005

**Files:**
- Modify: `src/store.rs:128-149` (replace the stale pre-rename test), add a new Profile-shaped round-trip test.

**Interfaces:**
- Consumes: `state::Profile`, `state::AppState` from Task 3.

- [ ] **Step 1: Write the failing test**

Replace `src/store.rs:128-149` (the `loading_a_state_file_written_before_backup_preference_existed_preserves_other_fields` test, which encodes the exact pre-rename shape ADR-0005 says should now reset) with:

```rust
    #[test]
    fn loading_a_pre_profile_rename_state_file_resets_to_default_per_adr_0005() {
        let path = temp_state_path("pre-profile-rename-upgrade");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"known_configs":["/home/user/.direwolf.conf"],"active_config":"/home/user/.direwolf.conf"}"#,
        )
        .unwrap();

        let state = StateStore::new(path).load();

        assert_eq!(state, AppState::default());
    }
```

- [ ] **Step 2: Update the surviving `state.add_known_config` call site**

Modify `src/store.rs:117-126` (`saved_state_survives_a_reload_via_a_new_store_instance`, which still uses the old method name — this is the test that already covers a Profile-shaped round trip, so nothing new needs adding for that):

```rust
    #[test]
    fn saved_state_survives_a_reload_via_a_new_store_instance() {
        let path = temp_state_path("roundtrip");
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/.direwolf.conf"), "Main".to_string(), false);

        StateStore::new(path.clone()).save(&state).unwrap();
        let reloaded = StateStore::new(path).load();

        assert_eq!(reloaded, state);
    }
```

- [ ] **Step 3: Run the store.rs tests**

Run: `cargo test --lib store::`
Expected: all `store::tests::*` pass, including the new ADR-0005 regression test.

- [ ] **Step 4: Run the full suite**

Run: `cargo build && cargo test`
Expected: builds and passes.

- [ ] **Step 5: Commit**

```bash
git add src/store.rs
git commit -m "$(cat <<'EOF'
Add regression coverage for ADR-0005's state.json reset behavior

A pre-Profile-rename state.json now has no `profiles` key, so deserialization
fails and DireUI falls back to AppState::default() rather than attempting to
interpret the old known_configs shape — this is the intended, documented
behavior (ADR-0005), not a bug; this test pins it down explicitly.
EOF
)"
```

---

### Task 5: Raw config Save feedback, no-change detection, and Cancel

**Files:**
- Modify: `src/main.rs` (`write_config` signature, `save_raw_config`, `clear_directive`'s call site, `index()`)
- Modify: `src/views.rs` (`raw_editor`, `profiles_page`'s flash rendering is already there from Task 3 — now `index()` actually passes `Some`)
- Modify: `assets/style.css` (flash banner, `.button`/`.button-row` for Cancel)

**Interfaces:**
- Consumes: `flash::Flash` from Task 2.
- Produces (used by Task 6): `write_config(ctx: &AppContext, path: &Path, content: String) -> Result<(), String>` (was `-> ()`).

- [ ] **Step 1: Write the failing view test for the new `raw_editor` signature**

Add to `src/views.rs`'s `tests` module (near the other simple rendering tests):

```rust
    #[test]
    fn raw_editor_has_no_error_message_by_default() {
        let html = raw_editor(Path::new("/home/user/.direwolf.conf"), "CHANNEL 0\n", None);

        assert!(!html.contains("save failed"));
    }

    #[test]
    fn raw_editor_shows_the_save_failed_reason_when_given_one() {
        let html = raw_editor(Path::new("/home/user/.direwolf.conf"), "CHANNEL 0\n", Some("disk full"));

        assert!(html.contains("save failed: disk full"));
    }

    #[test]
    fn raw_editor_has_a_cancel_link_back_to_profiles() {
        let html = raw_editor(Path::new("/home/user/.direwolf.conf"), "CHANNEL 0\n", None);

        assert!(html.contains(r#"<a class="button" href="/">Cancel</a>"#));
    }
```

- [ ] **Step 2: Run the tests to confirm they fail**

Run: `cargo test --lib views::tests::raw_editor`
Expected: FAIL — `raw_editor` takes 2 arguments today, not 3.

- [ ] **Step 3: Update `raw_editor`**

Replace `src/views.rs:212-223` (`raw_editor`):

```rust
pub fn raw_editor(path: &Path, content: &str, save_error: Option<&str>) -> String {
    let error_html = save_error
        .map(|reason| format!(r#"<p class="error-text">save failed: {}</p>"#, html_escape(reason)))
        .unwrap_or_default();
    format!(
        r#"<h1>Edit raw config</h1>
<p class="meta">Editing: <span class="config-path">{}</span></p>
{error}
<form method="post" action="/raw">
<textarea class="raw-editor" name="content">{}</textarea>
<div class="button-row">
<button type="submit">Save</button>
<a class="button" href="/">Cancel</a>
</div>
</form>"#,
        html_escape(&path.display().to_string()),
        html_escape(content),
        error = error_html
    )
}
```

- [ ] **Step 4: Update `raw_editor`'s one existing call site**

Modify `src/main.rs:140-148` (`edit_raw_config`):

```rust
async fn edit_raw_config(State(ctx): State<AppContext>) -> Html<String> {
    match read_active_config(&ctx) {
        Ok((path, content)) => Html(views::page(
            &views::raw_editor(&path, &content, None),
            Some(&path),
        )),
        Err(page) => page,
    }
}
```

- [ ] **Step 5: Run the view tests and confirm they pass**

Run: `cargo test --lib views::`
Expected: all pass, including the three new ones. (The crate as a whole still won't build yet — `write_config`/`save_raw_config` haven't been updated.)

- [ ] **Step 6: Change `write_config` to report failure instead of swallowing it**

Modify `src/main.rs:119-124`:

```rust
fn write_config(ctx: &AppContext, path: &std::path::Path, content: String) -> Result<(), String> {
    let backup_preference = backup_preference(ctx);
    backup::write_with_backup(path, &content, backup_preference).map_err(|err| {
        eprintln!("error: failed to save {}: {err}", path.display());
        err.to_string()
    })
}
```

- [ ] **Step 7: Rewrite `save_raw_config` with the compare/flash/error-render behavior**

Replace `src/main.rs:155-168`:

```rust
async fn save_raw_config(
    State(ctx): State<AppContext>,
    Form(form): Form<RawConfigForm>,
) -> axum::response::Response {
    let (path, current_content) = match read_active_config(&ctx) {
        Ok(pair) => pair,
        Err(page) => return page.into_response(),
    };
    // Browsers normalize textarea line breaks to CRLF on form submission
    // regardless of the file's original line endings — undo that so an
    // unedited save doesn't rewrite every line ending in the file, and so
    // this comparison isn't fooled by a line-ending-only "change".
    let submitted = form.content.replace("\r\n", "\n");

    if submitted == current_content {
        return Redirect::to(&format!("/?{}", flash::Flash::NoChange.to_query_string())).into_response();
    }

    match write_config(&ctx, &path, submitted.clone()) {
        Ok(()) => Redirect::to(&format!("/?{}", flash::Flash::Saved.to_query_string())).into_response(),
        Err(err) => Html(views::page(&views::raw_editor(&path, &submitted, Some(&err)), Some(&path)))
            .into_response(),
    }
}
```

- [ ] **Step 8: Fix `clear_directive`'s now-`Result`-returning `write_config` call**

Modify `src/main.rs:321-337` (`clear_directive`):

```rust
async fn clear_directive(
    State(ctx): State<AppContext>,
    Form(form): Form<ClearDirectiveForm>,
) -> axum::response::Response {
    let (path, content) = match read_active_config(&ctx) {
        Ok(pair) => pair,
        Err(page) => return page.into_response(),
    };

    if let Some(spec) = CURATED_FIELDS.iter().find(|s| s.name == form.clear_field) {
        let mut doc = config::Document::parse(&content);
        doc.clear_curated(spec.directive);
        // Clear's failure UX is out of scope for this change — same
        // best-effort, log-and-continue behavior as before write_config
        // started returning a Result.
        let _ = write_config(&ctx, &path, doc.serialize());
    }

    Redirect::to("/directives").into_response()
}
```

- [ ] **Step 9: Wire the flash param into `index()`**

Modify `src/main.rs:45-57`:

```rust
async fn index(
    State(ctx): State<AppContext>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Html<String> {
    let state = ctx.state.lock().unwrap();
    let flash = flash::Flash::from_params(&params);
    let body = if state.needs_first_run() {
        let suggestion = ctx
            .home
            .as_deref()
            .and_then(|home| state::suggest_default_config_path(home, |p| p.exists()));
        views::first_run(suggestion.as_deref())
    } else {
        views::profiles_page(&state, flash.as_ref())
    };
    Html(views::page(&body, state.active_config.as_deref()))
}
```

Modify `src/main.rs:13-19` (imports) to bring in `Query`:

```rust
use axum::{
    Form, Router,
    extract::{Query, State},
    http::header,
    response::{Html, IntoResponse, Redirect},
    routing::get,
};
```

- [ ] **Step 10: Add the flash banner and Cancel/button-row CSS**

Add to `assets/style.css`, after the `.error-text` rule (`assets/style.css:284-288`):

```css
.flash {
  margin: 0 0 1.25rem;
  padding: 0.6rem 0.9rem;
  border-radius: var(--radius);
  background: var(--alert-surface);
  color: var(--ink);
  border: 1px solid var(--line);
  font-size: 0.9rem;
}

.button-row {
  display: flex;
  gap: 0.5rem;
  align-items: center;
}

.button {
  display: inline-flex;
  align-items: center;
  font-family: var(--font-sans);
  font-size: 0.85rem;
  font-weight: 600;
  border-radius: var(--radius);
  padding: 0.5rem 0.9rem;
  border: 1px solid var(--line);
  background: var(--surface);
  color: var(--ink);
}

.button:hover {
  border-color: var(--ink-dim);
  text-decoration: none;
}
```

- [ ] **Step 11: Build and run the full suite**

Run: `cargo build && cargo test`
Expected: builds cleanly, all tests pass.

- [ ] **Step 12: Manually verify the raw-config Save flow**

Run: `cargo run` (in one terminal), then in another:

```bash
curl -s -c /tmp/direui-cookies -b /tmp/direui-cookies http://127.0.0.1:8080/ -o /dev/null -w '%{http_code}\n'
```

Then, in a browser: add a Profile, go to Edit raw config, click Save without changing anything → should redirect to `/` showing "no changes to save"; edit the text and Save → should redirect to `/` showing "changes saved"; click Cancel on the editor → should return to `/` immediately with no changes written.

- [ ] **Step 13: Commit**

```bash
git add src/main.rs src/views.rs assets/style.css
git commit -m "$(cat <<'EOF'
Add Save feedback, no-change detection, and Cancel to Edit raw config

Save now compares the submitted text against the Active Config's current
on-disk content before writing: identical content redirects to Profiles with
"no changes to save" instead of rewriting the file; a real change redirects
with "changes saved"; a write failure re-renders the editor in place with
"save failed: [reason]" and the user's unsaved edits intact. This also fixes
raw-config write failures being silently swallowed (only eprintln'd) before
this change. Cancel always discards and returns to Profiles.
EOF
)"
```

---

### Task 6: Directives Save feedback, no-change detection, and Cancel

**Files:**
- Modify: `src/main.rs` (`save_directives`)
- Modify: `src/views.rs` (`directives_editor`)

**Interfaces:**
- Consumes: `flash::Flash`, `write_config` (Task 5); `config::Document`'s existing `PartialEq`/`Clone` derives (already present, no `config.rs` change needed).

- [ ] **Step 1: Write the failing view tests for the new `directives_editor` signature**

Add to `src/views.rs`'s `tests` module:

```rust
    #[test]
    fn directives_editor_has_no_top_level_error_by_default() {
        let f = field("Audio device", "adevice");

        let html = directives_editor(std::slice::from_ref(&f), None);

        assert!(!html.contains("save failed"));
    }

    #[test]
    fn directives_editor_shows_a_top_level_save_failed_message_when_given_one() {
        let f = field("Audio device", "adevice");

        let html = directives_editor(std::slice::from_ref(&f), Some("disk full"));

        assert!(html.contains("save failed: disk full"));
    }

    #[test]
    fn directives_editor_has_a_cancel_link_back_to_profiles() {
        let f = field("Audio device", "adevice");

        let html = directives_editor(std::slice::from_ref(&f), None);

        assert!(html.contains(r#"<a class="button" href="/">Cancel</a>"#));
    }
```

- [ ] **Step 2: Run the tests to confirm they fail**

Run: `cargo test --lib views::tests::directives_editor`
Expected: FAIL — `directives_editor` takes 1 argument today, not 2.

- [ ] **Step 3: Update `directives_editor` and its other existing call-site tests**

Replace `src/views.rs:303-329` (`directives_editor`):

```rust
pub fn directives_editor(fields: &[DirectiveField], save_error: Option<&str>) -> String {
    let groups_html: String = fields
        .chunk_by(|a, b| a.group == b.group)
        .map(|group_fields| {
            let fields_html: String = group_fields.iter().map(directive_field_html).collect();
            format!(
                r#"<section class="field-group">
<h2 class="field-group-title">{}</h2>
<div class="panel">
{fields_html}
</div>
</section>
"#,
                html_escape(group_fields[0].group)
            )
        })
        .collect();

    let error_html = save_error
        .map(|reason| format!(r#"<p class="error-text">save failed: {}</p>"#, html_escape(reason)))
        .unwrap_or_default();

    format!(
        r#"<h1>Edit directives</h1>
{error}
<form method="post" action="/directives">
{}
<div class="button-row">
<button type="submit">Save</button>
<a class="button" href="/">Cancel</a>
</div>
</form>"#,
        groups_html,
        error = error_html
    )
}
```

Update the pre-existing `directives_editor` tests' call sites — every one currently calls `directives_editor(&fields)` or `directives_editor(std::slice::from_ref(&f))` with one argument; add `, None` to each:
`a_field_with_an_existing_value_gets_a_clear_button_targeting_its_own_field`, `a_field_with_no_existing_value_has_no_clear_button`, `a_field_marked_not_clearable_has_no_clear_button_even_with_a_value`, `multiline_fields_render_as_a_wrapping_textarea_not_a_single_line_input`, `directives_editor_renders_one_heading_per_group_in_order`, `directives_editor_places_each_field_within_its_group_section`.

- [ ] **Step 4: Run the view tests and confirm they pass**

Run: `cargo test --lib views::`
Expected: all pass.

- [ ] **Step 5: Rewrite `save_directives`**

Replace `src/main.rs:283-314`:

```rust
async fn save_directives(
    State(ctx): State<AppContext>,
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> axum::response::Response {
    let (path, content) = match read_active_config(&ctx) {
        Ok(pair) => pair,
        Err(page) => return page.into_response(),
    };
    let original_doc = config::Document::parse(&content);
    let mut doc = original_doc.clone();

    // All-or-nothing: validate every field before writing anything, so an
    // invalid PTT value can't leave a config with only some fields updated.
    let mut fields = Vec::new();
    let mut any_error = false;
    for spec in CURATED_FIELDS {
        let value = form.get(spec.name).cloned().unwrap_or_default();
        match doc.set_curated(spec.directive, &value) {
            Ok(()) => fields.push(spec.to_directive_field(value, None)),
            Err(err) => {
                any_error = true;
                fields.push(spec.to_directive_field(value, Some(err.message())));
            }
        }
    }

    if any_error {
        return Html(views::page(
            &views::directives_editor(&fields, Some("please correct the highlighted fields below")),
            Some(&path),
        ))
        .into_response();
    }

    if doc == original_doc {
        return Redirect::to(&format!("/?{}", flash::Flash::NoChange.to_query_string())).into_response();
    }

    match write_config(&ctx, &path, doc.serialize()) {
        Ok(()) => Redirect::to(&format!("/?{}", flash::Flash::Saved.to_query_string())).into_response(),
        Err(err) => {
            Html(views::page(&views::directives_editor(&fields, Some(&err)), Some(&path))).into_response()
        }
    }
}
```

- [ ] **Step 6: Update `edit_directives`'s call site**

Modify `src/main.rs:266-281` (`edit_directives`):

```rust
async fn edit_directives(State(ctx): State<AppContext>) -> Html<String> {
    match read_active_config(&ctx) {
        Ok((path, content)) => {
            let doc = config::Document::parse(&content);
            let fields: Vec<views::DirectiveField> = CURATED_FIELDS
                .iter()
                .map(|spec| {
                    let value = doc.get_curated(spec.directive).unwrap_or("").to_string();
                    spec.to_directive_field(value, None)
                })
                .collect();
            Html(views::page(&views::directives_editor(&fields, None), Some(&path)))
        }
        Err(page) => page,
    }
}
```

- [ ] **Step 7: Build and run the full suite**

Run: `cargo build && cargo test`
Expected: builds cleanly, all tests pass.

- [ ] **Step 8: Manually verify the directives Save flow**

With `cargo run` running: go to Edit directives, click Save with no changes → "no changes to save" on Profiles; change a field and Save → "changes saved"; submit an invalid value (e.g. a non-numeric CHANNEL) → per-field error stays, plus "save failed: please correct the highlighted fields below" at the top, no redirect; click Cancel → returns to Profiles immediately, no changes written.

- [ ] **Step 9: Commit**

```bash
git add src/main.rs src/views.rs
git commit -m "$(cat <<'EOF'
Add Save feedback, no-change detection, and Cancel to Edit directives

Compares the built Document against the on-disk parsed Document (structural
equality, so this stays compatible with ADR-0001's round-trip preservation)
before writing. Validation failures keep their existing per-field messages
and now also show a top-level "save failed" message. Cancel always discards
and returns to Profiles.
EOF
)"
```

---

### Task 7: Rename a Profile inline

**Files:**
- Modify: `src/main.rs` (`rename_profile` handler + route)
- Modify: `src/views.rs` (`profile_row_html` gains the pencil-edit affordance)
- Create: `assets/app.js` (rename-toggle script; delete-dialog script is added in Task 8, same file)
- Modify: `src/main.rs` (serve `assets/app.js`, matching the existing `htmx_js`/`style_css` pattern)
- Modify: `assets/style.css` (`.profile-name-cell`, `.profile-rename-form`, `.icon-button`)

**Interfaces:**
- Consumes: `AppState::rename_profile` (Task 3).

- [ ] **Step 1: Write the failing test for the rename form markup**

Add to `src/views.rs`'s `tests` module:

```rust
    #[test]
    fn profiles_page_includes_a_rename_form_for_each_profile() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/aprs.conf"), "APRS".to_string(), false);

        let html = profiles_page(&state, None);

        assert!(html.contains(r#"<form method="post" action="/profiles/rename""#));
        assert!(html.contains(r#"value="/home/user/aprs.conf""#));
        assert!(html.contains(r#"value="APRS""#));
    }
```

- [ ] **Step 2: Run the test to confirm it fails**

Run: `cargo test --lib views::tests::profiles_page_includes_a_rename_form_for_each_profile`
Expected: FAIL — no `/profiles/rename` form exists yet.

- [ ] **Step 3: Add the rename affordance to `profile_row_html`**

Replace `profile_row_html` in `src/views.rs` (added in Task 3):

```rust
fn profile_row_html(profile: &state::Profile, is_active: bool, path_html: &str) -> String {
    let name = html_escape(&profile.name);
    let path_attr = html_escape(&profile.path.display().to_string());
    let status_html = if is_active {
        r#"<span class="pill profile-badge">active</span>"#.to_string()
    } else {
        format!(
            r#"<form method="post" action="/profiles/activate"><input type="hidden" name="path" value="{path}"><button type="submit">Switch to this</button></form>"#,
            path = path_attr
        )
    };
    format!(
        r#"<li>
<div class="profile-main">
<div class="profile-name-cell">
<span class="profile-name" data-name-display>{name}</span>
<form method="post" action="/profiles/rename" class="profile-rename-form" data-name-edit hidden>
<input type="hidden" name="path" value="{path}">
<input type="text" name="name" value="{name}" required>
<button type="submit">Save</button>
<button type="button" data-cancel-rename>Cancel</button>
</form>
<button type="button" class="icon-button" data-edit-name aria-label="Rename profile">&#9998;</button>
</div>
<span class="config-path" title="{path}">{path_html}</span>
</div>
<div class="profile-actions">
{status}
</div>
</li>"#,
        name = name,
        path = path_attr,
        path_html = path_html,
        status = status_html
    )
}
```

(The delete trashcan button is added to `.profile-actions` in Task 8 — this task only adds the name/rename cell and wraps the existing status control in `.profile-main`/`.profile-actions` so Task 8 has somewhere to put it.)

- [ ] **Step 4: Run the view tests and confirm they pass**

Run: `cargo test --lib views::`
Expected: all pass, including the new one.

- [ ] **Step 5: Add the `rename_profile` handler and route**

Modify `src/main.rs`, adding near `activate_profile`:

```rust
#[derive(Deserialize)]
struct RenameProfileForm {
    path: String,
    name: String,
}

async fn rename_profile(State(ctx): State<AppContext>, Form(form): Form<RenameProfileForm>) -> Redirect {
    let path = PathBuf::from(form.path);
    ctx.mutate_and_save(|state| {
        if let Err(err) = state.rename_profile(&path, form.name) {
            eprintln!("error: {err}");
        }
    });
    Redirect::to("/")
}
```

Modify the router (adjacent to `/profiles/activate`):

```rust
        .route("/profiles/rename", axum::routing::post(rename_profile))
```

- [ ] **Step 6: Add and serve `assets/app.js`**

Create `assets/app.js`:

```js
document.querySelectorAll('[data-edit-name]').forEach(function (btn) {
  btn.addEventListener('click', function () {
    var row = btn.closest('li');
    row.querySelector('[data-name-display]').hidden = true;
    row.querySelector('[data-name-edit]').hidden = false;
    btn.hidden = true;
  });
});

document.querySelectorAll('[data-cancel-rename]').forEach(function (btn) {
  btn.addEventListener('click', function () {
    var row = btn.closest('li');
    row.querySelector('[data-name-display]').hidden = false;
    row.querySelector('[data-name-edit]').hidden = true;
    row.querySelector('[data-edit-name]').hidden = false;
  });
});
```

Modify `src/main.rs:25-26` (asset consts):

```rust
const HTMX_JS: &[u8] = include_bytes!("../assets/vendor/htmx/htmx.min.js");
const APP_JS: &[u8] = include_bytes!("../assets/app.js");
const STYLE_CSS: &[u8] = include_bytes!("../assets/style.css");
```

Modify `src/main.rs:59-65` (adjacent to `htmx_js`/`style_css`):

```rust
async fn htmx_js() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], HTMX_JS)
}

async fn app_js() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], APP_JS)
}

async fn style_css() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css")], STYLE_CSS)
}
```

Modify the router to serve it:

```rust
        .route("/vendor/htmx/htmx.min.js", get(htmx_js))
        .route("/app.js", get(app_js))
        .route("/style.css", get(style_css))
```

Modify `src/views.rs:20-28` (`page()`'s `<head>`) to load it:

```rust
<link rel="stylesheet" href="/style.css">
<script src="/vendor/htmx/htmx.min.js"></script>
<script src="/app.js" defer></script>
```

- [ ] **Step 7: Add the rename/name-cell CSS**

Add to `assets/style.css`, after the `.profile-name`/`.checkbox-label` rules added in Task 3:

```css
.profile-main {
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
}

.profile-name-cell {
  display: flex;
  align-items: center;
  gap: 0.4rem;
}

.profile-rename-form {
  display: flex;
  align-items: center;
  gap: 0.4rem;
}

.profile-rename-form input[type="text"] {
  width: auto;
  flex: initial;
}

.profile-actions {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.icon-button {
  background: transparent;
  border: 1px solid transparent;
  font-size: 1rem;
  line-height: 1;
  padding: 0.3rem;
}

.icon-button:hover {
  border-color: var(--line);
}
```

- [ ] **Step 8: Build and run the full suite**

Run: `cargo build && cargo test`
Expected: builds cleanly, all tests pass.

- [ ] **Step 9: Manually verify renaming**

With `cargo run` running: on the Profiles page, click the pencil icon next to a Profile's name — it should turn into an editable text field with Save/Cancel; typing a new name and clicking Save should redirect to `/` showing the new name in the (correct, active-first/Last-Activated-ordered) position; clicking the inline Cancel (before submitting) should revert to the display span without a page reload.

- [ ] **Step 10: Commit**

```bash
git add src/main.rs src/views.rs assets/app.js assets/style.css
git commit -m "$(cat <<'EOF'
Add inline Profile renaming

A pencil icon next to each Profile's name toggles it into an editable field
(pure client-side toggle, no server round-trip for the toggle itself); saving
posts to the new /profiles/rename route and returns to the Profiles page.
EOF
)"
```

---

### Task 8: Delete a Profile (Remove vs. Delete)

**Files:**
- Modify: `src/main.rs` (`remove_profile`, `delete_profile` handlers + routes)
- Modify: `src/views.rs` (trashcan button in `profile_row_html`, the shared delete `<dialog>` markup, wired into `profiles_page`)
- Modify: `assets/app.js` (delete-dialog script)
- Modify: `assets/style.css` (`dialog`, `.dialog-actions`, `.button-danger`)

**Interfaces:**
- Consumes: `AppState::remove_profile` (Task 3), `flash::Flash::Removed`/`Deleted`/`DeleteFailed` (Task 2).

- [ ] **Step 1: Write the failing tests for the trashcan button and dialog markup**

Add to `src/views.rs`'s `tests` module:

```rust
    #[test]
    fn profiles_page_includes_a_delete_button_for_each_profile() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/aprs.conf"), "APRS".to_string(), false);

        let html = profiles_page(&state, None);

        assert!(html.contains(r#"data-delete-path="/home/user/aprs.conf""#));
        assert!(html.contains(r#"data-delete-name="APRS""#));
    }

    #[test]
    fn profiles_page_includes_the_shared_delete_dialog_with_both_actions() {
        let state = AppState::default();

        let html = profiles_page(&state, None);

        assert!(html.contains(r#"<dialog id="delete-dialog""#));
        assert!(html.contains(r#"action="/profiles/remove""#));
        assert!(html.contains(r#"action="/profiles/delete""#));
    }
```

- [ ] **Step 2: Run the tests to confirm they fail**

Run: `cargo test --lib views::tests::profiles_page_includes_a_delete_button_for_each_profile views::tests::profiles_page_includes_the_shared_delete_dialog_with_both_actions`
Expected: FAIL — neither the trashcan button nor the dialog exist yet.

- [ ] **Step 3: Add the trashcan button to `profile_row_html`**

Modify the `.profile-actions` block inside `profile_row_html` (from Task 7):

```rust
<div class="profile-actions">
{status}
<button type="button" class="icon-button" data-delete-path="{path}" data-delete-name="{name}" aria-label="Delete profile">&#128465;</button>
</div>
```

- [ ] **Step 4: Add the shared delete dialog markup**

Add a new function to `src/views.rs`, near `profiles_page`:

```rust
fn delete_dialog() -> String {
    r#"<dialog id="delete-dialog" class="delete-dialog">
<div id="delete-step-1">
<p>Remove <strong id="delete-dialog-name"></strong> from DireUI?</p>
<div class="dialog-actions">
<button type="button" onclick="document.getElementById('delete-dialog').close()">Cancel</button>
<form method="post" action="/profiles/remove">
<input type="hidden" name="path" id="delete-dialog-remove-path">
<button type="submit">Remove</button>
</form>
<button type="button" id="delete-dialog-show-step-2">Delete file too&hellip;</button>
</div>
</div>
<div id="delete-step-2" hidden>
<p class="error-text">This permanently deletes the Config File from disk. This cannot be undone. If this is the active Profile, another Profile will automatically become active.</p>
<div class="dialog-actions">
<button type="button" id="delete-dialog-show-step-1">Back</button>
<form method="post" action="/profiles/delete">
<input type="hidden" name="path" id="delete-dialog-delete-path">
<button type="submit" class="button-danger">Yes, delete the file</button>
</form>
</div>
</div>
</dialog>"#
        .to_string()
}
```

Wire it into `profiles_page`'s output (`src/views.rs`, the format string from Task 3), adding `{dialog}` after the `</nav>`:

```rust
    format!(
        r##"<h1>Profiles</h1>
{flash}
<ul class="profile-list">{items}</ul>
{add_form}
{backup_toggle}
<nav class="actions">
<a href="/directives">Edit directives</a>
<a href="/raw">Edit raw config</a>
<div class="status-check">
<button hx-get="/status" hx-target="#server-status" hx-swap="innerHTML">Check server status</button>
<span id="server-status" aria-live="polite"></span>
</div>
</nav>
{dialog}"##,
        flash = flash_html,
        items = list_items,
        add_form = add_profile_form("", "/home/user/aprs.conf", "e.g. APRS", "Add profile", !state.profiles.is_empty()),
        backup_toggle = backup_preference_toggle(state.backup_preference),
        dialog = delete_dialog(),
    )
```

- [ ] **Step 5: Run the view tests and confirm they pass**

Run: `cargo test --lib views::`
Expected: all pass.

- [ ] **Step 6: Add the `remove_profile`/`delete_profile` handlers and routes**

Modify `src/main.rs`, adding near `rename_profile`:

```rust
async fn remove_profile(State(ctx): State<AppContext>, Form(form): Form<PathForm>) -> Redirect {
    let path = PathBuf::from(form.path);
    ctx.mutate_and_save(|state| state.remove_profile(&path));
    Redirect::to(&format!("/?{}", flash::Flash::Removed.to_query_string()))
}

async fn delete_profile(State(ctx): State<AppContext>, Form(form): Form<PathForm>) -> Redirect {
    let path = PathBuf::from(form.path);
    if let Err(err) = std::fs::remove_file(&path) {
        eprintln!("error: failed to delete {}: {err}", path.display());
        return Redirect::to(&format!(
            "/?{}",
            flash::Flash::DeleteFailed(err.to_string()).to_query_string()
        ));
    }
    ctx.mutate_and_save(|state| state.remove_profile(&path));
    Redirect::to(&format!("/?{}", flash::Flash::Deleted.to_query_string()))
}
```

Modify the router:

```rust
        .route("/profiles/remove", axum::routing::post(remove_profile))
        .route("/profiles/delete", axum::routing::post(delete_profile))
```

- [ ] **Step 7: Add the delete-dialog script**

Append to `assets/app.js`:

```js
function openDeleteDialog(path, name) {
  document.getElementById('delete-dialog-name').textContent = name;
  document.getElementById('delete-dialog-remove-path').value = path;
  document.getElementById('delete-dialog-delete-path').value = path;
  showDeleteStep(1);
  document.getElementById('delete-dialog').showModal();
}

function showDeleteStep(n) {
  document.getElementById('delete-step-1').hidden = n !== 1;
  document.getElementById('delete-step-2').hidden = n !== 2;
}

document.querySelectorAll('[data-delete-path]').forEach(function (btn) {
  btn.addEventListener('click', function () {
    openDeleteDialog(btn.dataset.deletePath, btn.dataset.deleteName);
  });
});

var showStep2Button = document.getElementById('delete-dialog-show-step-2');
if (showStep2Button) {
  showStep2Button.addEventListener('click', function () {
    showDeleteStep(2);
  });
}

var showStep1Button = document.getElementById('delete-dialog-show-step-1');
if (showStep1Button) {
  showStep1Button.addEventListener('click', function () {
    showDeleteStep(1);
  });
}
```

- [ ] **Step 8: Add the dialog/danger-button CSS**

Add to `assets/style.css`:

```css
dialog.delete-dialog {
  border: 1px solid var(--line);
  border-radius: var(--radius);
  padding: 1.25rem;
  max-width: 420px;
  background: var(--surface);
  color: var(--ink);
}

dialog.delete-dialog::backdrop {
  background: rgba(0, 0, 0, 0.4);
}

.dialog-actions {
  display: flex;
  gap: 0.5rem;
  margin-top: 1rem;
}

.button-danger {
  background: var(--alert);
  color: var(--signal-ink);
  border-color: var(--alert);
}
```

- [ ] **Step 9: Build and run the full suite**

Run: `cargo build && cargo test`
Expected: builds cleanly, all tests pass.

- [ ] **Step 10: Manually verify deletion**

With `cargo run` running and at least two Profiles added (one of them the active one): click the trashcan on the non-active Profile → dialog shows its name → click Remove → returns to `/` showing "Profile removed", row gone, file still on disk. Click the trashcan on the active Profile → dialog should still open (deleting the active Profile is allowed) → click "Delete file too…" → step 2's warning appears → click "Yes, delete the file" → returns to `/` showing "Profile and file deleted", the file is actually gone from disk, and another Profile has automatically become active (check the persistent header). Try deleting a file DireUI can't remove (e.g. `chmod` the containing directory read-only) → should show "couldn't delete file: [os error]" and the Profile record should remain (not removed on a failed file delete).

- [ ] **Step 11: Commit**

```bash
git add src/main.rs src/views.rs assets/app.js assets/style.css
git commit -m "$(cat <<'EOF'
Add Profile deletion: Remove (DireUI only) vs Delete (also removes the file)

A trashcan icon opens a shared dialog with three choices: cancel, Remove
(deletes the Profile record only), or a second-confirmed Delete (also
removes the Config File from disk, irreversible). Deleting the active
Profile is allowed — DireUI auto-promotes the next Profile by Last Activated
so there's never a Profile list with nothing active.
EOF
)"
```

---

### Task 9: Final verification

**Files:** none (verification only)

- [ ] **Step 1: Run the full test suite one more time**

Run: `cargo build && cargo test`
Expected: builds cleanly, all tests pass, zero warnings (`cargo build 2>&1 | grep -i warning` should print nothing).

- [ ] **Step 2: Confirm every renamed/removed identifier is actually gone**

Run: `grep -rn "known_config\|config_manager\|add_config\b\|set_active_config\|/configs\b" --include="*.rs" src/`
Expected: no output (everything from the "Configurations" era is renamed).

- [ ] **Step 3: Full manual walkthrough**

With `cargo run` running, exercise the whole flow end to end in a browser: fresh `state.json` (delete `~/.config/direui/state.json` first, or point `XDG_CONFIG_HOME` at a scratch dir) → first-run add-profile form requires both path and name, no "make active" checkbox shown → after adding, land on Profiles with exactly one Profile, active, first row → add a second Profile without checking "make active" → it appears second, active Profile stays first → rename either Profile inline → order updates correctly (renaming doesn't change Last Activated) → switch active to the second Profile → it moves to row 1 → edit directives, Save with a change → "changes saved" → edit raw config, Save with no change → "no changes to save" → Cancel on each editor → returns immediately, no writes → delete the (now non-active) first Profile via Remove → gone, file untouched → delete the active Profile via Delete → file removed from disk, another Profile auto-promoted to active.

- [ ] **Step 4: Close the loop on the tracking issue**

```bash
gh issue comment <issue-number-from-task-1> --body "Implemented: full rename to Profiles, named/ordered list, inline rename, Remove vs Delete, Save feedback + no-change detection + Cancel on both editors. Closing — remaining v2 scope (process control, live status, dry-run validation, Backup History versioning) stays on #13."
gh issue close <issue-number-from-task-1>
```
