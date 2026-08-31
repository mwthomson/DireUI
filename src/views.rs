use std::path::Path;

use crate::state::{self, AppState};

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// Present on every page so a config being edited elsewhere (raw-text editor,
// Curated Directive form) never loses sight of which Config File it's
// editing, and always has a consistent way back to the config manager.
pub fn page(body: &str, active_config: Option<&Path>) -> String {
    let active = active_config
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "none".to_string());
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>DireUI</title>
<link rel="stylesheet" href="/style.css">
<script src="/vendor/htmx/htmx.min.js"></script>
<script src="/app.js" defer></script>
</head>
<body>
<header class="site-header">
<p class="wordmark">DireUI</p>
<nav class="site-nav">
<span class="site-active-config">Active config: <span class="config-path">{active}</span></span>
<a href="/">Profiles</a>
</nav>
</header>
<main class="page">
{body}
</main>
</body>
</html>
"#,
        active = html_escape(&active),
        body = body
    )
}

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

fn backup_preference_toggle(enabled: bool) -> String {
    let status = if enabled { "on" } else { "off" };
    let next_value = if enabled { "false" } else { "true" };
    let button_label = if enabled { "Turn off" } else { "Turn on" };
    format!(
        r#"<form method="post" action="/backup-preference" class="form-inline">
<span>Backup before save: {status}</span>
<input type="hidden" name="enabled" value="{next_value}">
<button type="submit">{button_label}</button>
</form>"#
    )
}

// Counts how many leading elements are the same, position by position,
// across every sequence, stopping after `limit` positions.
fn count_matching_prefix(sequences: &[Vec<&str>], limit: usize) -> usize {
    (0..limit)
        .take_while(|&i| sequences.iter().all(|s| s[i] == sequences[0][i]))
        .count()
}

// Splits each path on '/' and finds the run of leading and trailing segments
// shared by every path in the list, leaving whatever's left in the middle as
// the part that actually differs between them. This is a positional
// heuristic (aligned by segment index from each end), not a full diff — with
// paths of differing directory depth it can sweep a shared segment name into
// the "differing" middle if its position shifted, but it never misrenders a
// path, and it correctly isolates the differing segment(s) for the common
// case this mitigation targets: same-depth paths differing in one component.
fn common_segment_range(segments: &[Vec<&str>]) -> (usize, usize) {
    if segments.len() < 2 {
        return (segments.first().map_or(0, |s| s.len()), 0);
    }

    let min_len = segments.iter().map(|s| s.len()).min().unwrap();
    let prefix_len = count_matching_prefix(segments, min_len);

    let max_suffix = min_len - prefix_len;
    let reversed: Vec<Vec<&str>> = segments
        .iter()
        .map(|s| s.iter().rev().copied().collect())
        .collect();
    let suffix_len = count_matching_prefix(&reversed, max_suffix);

    (prefix_len, suffix_len)
}

// With more than one saved config path, long, mostly-identical paths (e.g.
// two entries differing only in one directory name) are hard to tell apart
// at a glance. Wraps the segment(s) that actually differ across `paths` in a
// `config-path-diff` span so the difference stands out without reading the
// full path character by character. Output is HTML-escaped and safe to
// inline directly.
fn highlight_differing_segments(paths: &[String]) -> Vec<String> {
    let segments: Vec<Vec<&str>> = paths.iter().map(|p| p.split('/').collect()).collect();
    let (prefix_len, suffix_len) = common_segment_range(&segments);

    segments
        .iter()
        .map(|s| {
            let suffix_start = s.len() - suffix_len;
            let groups: [(&[&str], Option<&str>); 3] = [
                (&s[..prefix_len], None),
                (&s[prefix_len..suffix_start], Some("config-path-diff")),
                (&s[suffix_start..], None),
            ];

            let mut html = String::new();
            let mut first = true;
            for (group, class) in groups {
                if group.is_empty() {
                    continue;
                }
                if !first {
                    html.push('/');
                }
                first = false;
                let text = html_escape(&group.join("/"));
                match class {
                    Some(c) => html.push_str(&format!(r#"<span class="{c}">{text}</span>"#)),
                    None => html.push_str(&text),
                }
            }
            html
        })
        .collect()
}

// Swapped into #server-status by the "Check server status" button. A pill
// rather than plain text so a status result reads as status, not prose.
pub fn status_indicator() -> String {
    r#"<span class="pill status-pill status-ok">Server is running</span>"#.to_string()
}

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
<button type="button" class="icon-button" data-delete-path="{path}" data-delete-name="{name}" aria-label="Delete profile">&#128465;</button>
</div>
</li>"#,
        name = name,
        path = path_attr,
        path_html = path_html,
        status = status_html
    )
}

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
</nav>
{dialog}"##,
        flash = flash_html,
        items = list_items,
        add_form = add_profile_form("", "/home/user/aprs.conf", "e.g. APRS", "Add profile", !state.profiles.is_empty()),
        backup_toggle = backup_preference_toggle(state.backup_preference),
        dialog = delete_dialog(),
    )
}

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

pub struct DirectiveField {
    pub label: &'static str,
    pub name: &'static str,
    pub group: &'static str,
    // Directives whose values can run long (e.g. PBEACON's full parameter
    // string) render as a wrapping textarea instead of a single-line input,
    // so the whole value stays visible rather than scrolling off-screen.
    pub multiline: bool,
    // False for directives where removing the line has consequences beyond
    // that directive itself (e.g. CHANNEL scopes the MODEM/PTT lines under
    // it) — the Clear button is withheld rather than offering an action
    // that can silently orphan other lines.
    pub clearable: bool,
    pub value: String,
    pub error: Option<&'static str>,
}

// Field labels carry their raw Direwolf directive keyword as a trailing
// "(KEYWORD)" (e.g. "Audio device (ADEVICE)") — split it out so it can
// render as its own badge rather than as part of the label's prose.
fn split_label(label: &str) -> (&str, Option<&str>) {
    match label.strip_suffix(')').and_then(|s| s.rsplit_once(" (")) {
        Some((prefix, keyword)) => (prefix, Some(keyword)),
        None => (label, None),
    }
}

fn directive_field_html(f: &DirectiveField) -> String {
    let (label_text, keyword) = split_label(f.label);
    let badge_html = keyword
        .map(|k| format!(r#"<span class="field-badge">{}</span>"#, html_escape(k)))
        .unwrap_or_default();
    let error_html = f
        .error
        .map(|msg| format!(r#"<p class="error-text">{}</p>"#, html_escape(msg)))
        .unwrap_or_default();
    let input_html = if f.multiline {
        format!(
            r#"<textarea class="field-textarea" id="{name}" name="{name}" rows="2">{value}</textarea>"#,
            name = f.name,
            value = html_escape(&f.value)
        )
    } else {
        format!(
            r#"<input type="text" id="{name}" name="{name}" value="{value}">"#,
            name = f.name,
            value = html_escape(&f.value)
        )
    };
    // Only a directive with an existing value can be cleared — a never-set
    // field has nothing to remove from the Config File.
    let clear_html = if f.value.is_empty() || !f.clearable {
        String::new()
    } else {
        format!(
            r#"<button type="submit" formaction="/directives/clear" name="clear_field" value="{name}" class="field-clear">Clear</button>"#,
            name = f.name
        )
    };
    format!(
        r#"<div class="field">
<label class="field-label" for="{name}">{label}{badge}</label>
{input}
{clear}
{error}
</div>"#,
        name = f.name,
        label = html_escape(label_text),
        badge = badge_html,
        input = input_html,
        clear = clear_html,
        error = error_html
    )
}

// Fields already arrive ordered by directive area (see CURATED_FIELDS), so
// grouping is a matter of chunking on consecutive equal `group` values
// rather than sorting/bucketing by group name.
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

pub fn no_active_config() -> String {
    r#"<h1>No active config</h1>
<p>Add a config file before editing.</p>"#
        .to_string()
}

pub fn error(message: &str) -> String {
    format!(
        r#"<h1>Error</h1>
<p class="error-text">{}</p>"#,
        html_escape(message)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Profile;
    use std::path::PathBuf;

    #[test]
    fn status_indicator_renders_as_a_distinct_visual_pill_not_plain_text() {
        let html = status_indicator();

        assert!(html.contains("status-pill"));
    }

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

    #[test]
    fn profiles_page_status_button_targets_a_separate_indicator_not_itself() {
        let state = AppState::default();

        let html = profiles_page(&state, None);

        // The button must survive the swap so it can be clicked again — it
        // targets a sibling element rather than replacing itself.
        assert!(html.contains(
            r##"<button hx-get="/status" hx-target="#server-status" hx-swap="innerHTML">Check server status</button>"##
        ));
        assert!(html.contains(r#"<span id="server-status""#));
    }

    #[test]
    fn profiles_page_includes_a_rename_form_for_each_profile() {
        let mut state = AppState::default();
        state.add_profile(PathBuf::from("/home/user/aprs.conf"), "APRS".to_string(), false);

        let html = profiles_page(&state, None);

        assert!(html.contains(r#"<form method="post" action="/profiles/rename""#));
        assert!(html.contains(r#"value="/home/user/aprs.conf""#));
        assert!(html.contains(r#"value="APRS""#));
    }

    #[test]
    fn page_shows_the_active_config_path_in_the_persistent_header() {
        let html = page(
            "<p>body</p>",
            Some(Path::new("/home/pi/aprs-config/direwolf.conf")),
        );

        assert!(html.contains("/home/pi/aprs-config/direwolf.conf"));
    }

    #[test]
    fn page_shows_none_when_there_is_no_active_config() {
        let html = page("<p>body</p>", None);

        assert!(html.contains("none"));
    }

    #[test]
    fn page_always_links_back_to_the_profiles_page() {
        let html = page("<p>body</p>", None);

        assert!(html.contains(r#"<a href="/">Profiles</a>"#));
    }

    #[test]
    fn page_escapes_html_in_the_active_config_path() {
        let html = page(
            "<p>body</p>",
            Some(Path::new("/home/<script>/direwolf.conf")),
        );

        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn highlight_differing_segments_marks_only_the_segment_that_differs() {
        let paths = [
            "/home/pi/aprs-config/direwolf.conf".to_string(),
            "/home/pi/packet-config/direwolf.conf".to_string(),
        ];

        let html = highlight_differing_segments(&paths);

        assert_eq!(
            html[0],
            r#"/home/pi/<span class="config-path-diff">aprs-config</span>/direwolf.conf"#
        );
        assert_eq!(
            html[1],
            r#"/home/pi/<span class="config-path-diff">packet-config</span>/direwolf.conf"#
        );
    }

    #[test]
    fn highlight_differing_segments_leaves_a_single_path_unmarked() {
        let paths = ["/home/pi/.direwolf.conf".to_string()];

        let html = highlight_differing_segments(&paths);

        assert_eq!(html[0], "/home/pi/.direwolf.conf");
        assert!(!html[0].contains("config-path-diff"));
    }

    #[test]
    fn highlight_differing_segments_marks_nothing_for_identical_paths() {
        let paths = [
            "/home/pi/.direwolf.conf".to_string(),
            "/home/pi/.direwolf.conf".to_string(),
        ];

        let html = highlight_differing_segments(&paths);

        assert!(!html[0].contains("config-path-diff"));
        assert!(!html[1].contains("config-path-diff"));
    }

    #[test]
    fn highlight_differing_segments_marks_the_whole_path_when_nothing_is_shared() {
        let paths = ["/aaa/one.conf".to_string(), "/ccc/two.conf".to_string()];

        let html = highlight_differing_segments(&paths);

        assert_eq!(
            html[0],
            r#"/<span class="config-path-diff">aaa/one.conf</span>"#
        );
        assert_eq!(
            html[1],
            r#"/<span class="config-path-diff">ccc/two.conf</span>"#
        );
    }

    #[test]
    fn highlight_differing_segments_still_renders_correctly_when_paths_differ_in_depth() {
        // `common_segment_range` is a positional heuristic: paths of
        // different depth can shift a shared segment name (here "pi") out
        // of alignment, sweeping it into the highlighted middle instead of
        // recognizing it as shared. The highlight is broader than a true
        // diff would produce, but every path still reconstructs byte-for-
        // byte, and the real point of difference is still covered.
        let paths = [
            "/home/pi/aprs/direwolf.conf".to_string(),
            "/home/extra/pi/packet/direwolf.conf".to_string(),
        ];

        let html = highlight_differing_segments(&paths);

        assert_eq!(
            html[0],
            r#"/home/<span class="config-path-diff">pi/aprs</span>/direwolf.conf"#
        );
        assert_eq!(
            html[1],
            r#"/home/<span class="config-path-diff">extra/pi/packet</span>/direwolf.conf"#
        );
    }

    #[test]
    fn highlight_differing_segments_escapes_html_in_paths() {
        let paths = [
            "/home/<a>/direwolf.conf".to_string(),
            "/home/<b>/direwolf.conf".to_string(),
        ];

        let html = highlight_differing_segments(&paths);

        assert_eq!(
            html[0],
            r#"/home/<span class="config-path-diff">&lt;a&gt;</span>/direwolf.conf"#
        );
    }

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

    #[test]
    fn profiles_page_still_marks_the_active_config_when_paths_are_similar() {
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
            active_config: Some(PathBuf::from("/home/pi/packet-config/direwolf.conf")),
            backup_preference: false,
        };

        let html = profiles_page(&state, None);

        assert!(html.contains(r#"<span class="pill profile-badge">active</span>"#));
        assert!(html.contains(r#"<span class="config-path-diff">packet-config</span>"#));
    }

    #[test]
    fn profiles_page_shows_the_full_path_as_a_title_attribute() {
        let state = AppState {
            profiles: vec![Profile {
                path: PathBuf::from("/home/pi/aprs-config/direwolf.conf"),
                name: "APRS".to_string(),
                last_activated_at: 0,
            }],
            active_config: Some(PathBuf::from("/home/pi/aprs-config/direwolf.conf")),
            backup_preference: false,
        };

        let html = profiles_page(&state, None);

        assert!(html.contains(r#"title="/home/pi/aprs-config/direwolf.conf""#));
    }

    fn field(group: &'static str, name: &'static str) -> DirectiveField {
        DirectiveField {
            label: name,
            name,
            group,
            multiline: false,
            clearable: true,
            value: String::new(),
            error: None,
        }
    }

    #[test]
    fn a_field_with_an_existing_value_gets_a_clear_button_targeting_its_own_field() {
        let mut f = field("APRS beaconing", "cbeacon");
        f.value = "delay=1 info=\"Test\"".to_string();

        let html = directives_editor(std::slice::from_ref(&f), None);

        // formaction submits the same form to a separate route, so clearing
        // is a deliberate action distinct from blanking the field and
        // clicking Save.
        assert!(html.contains(
            r#"<button type="submit" formaction="/directives/clear" name="clear_field" value="cbeacon" class="field-clear">Clear</button>"#
        ));
    }

    #[test]
    fn a_field_with_no_existing_value_has_no_clear_button() {
        let f = field("APRS beaconing", "cbeacon");

        let html = directives_editor(std::slice::from_ref(&f), None);

        assert!(!html.contains("Clear"));
    }

    #[test]
    fn a_field_marked_not_clearable_has_no_clear_button_even_with_a_value() {
        // CHANNEL is a repeating scope selector for MODEM/PTT lines beneath
        // it (see the NOTE on config::Document::get_curated) — clearing it
        // could silently orphan those scoped lines, so it's excluded.
        let mut f = field("Channel, modem & PTT", "channel");
        f.clearable = false;
        f.value = "0".to_string();

        let html = directives_editor(std::slice::from_ref(&f), None);

        assert!(!html.contains("Clear"));
    }

    #[test]
    fn multiline_fields_render_as_a_wrapping_textarea_not_a_single_line_input() {
        let mut f = field("APRS beaconing", "pbeacon");
        f.multiline = true;
        f.value = "delay=1 every=30 lat=42^37.14N long=071^20.83W".to_string();

        let html = directives_editor(std::slice::from_ref(&f), None);

        assert!(html.contains(r#"<textarea class="field-textarea" id="pbeacon" name="pbeacon" rows="2">delay=1 every=30 lat=42^37.14N long=071^20.83W</textarea>"#));
        assert!(!html.contains(r#"<input type="text" id="pbeacon""#));
    }

    #[test]
    fn directives_editor_renders_one_heading_per_group_in_order() {
        let fields = [
            field("Audio device", "adevice"),
            field("Channel, modem & PTT", "channel"),
            field("Channel, modem & PTT", "modem"),
            field("Channel, modem & PTT", "ptt"),
        ];

        let html = directives_editor(&fields, None);

        let audio_pos = html.find("Audio device").unwrap();
        let channel_pos = html.find("Channel, modem &amp; PTT").unwrap();
        assert!(audio_pos < channel_pos);
        // Exactly one heading per group, not one per field.
        assert_eq!(html.matches("Channel, modem &amp; PTT").count(), 1);
    }

    #[test]
    fn directives_editor_places_each_field_within_its_group_section() {
        let fields = [field("Audio device", "adevice"), field("Network ports", "agwport")];

        let html = directives_editor(&fields, None);

        let audio_heading = html.find("Audio device").unwrap();
        let adevice_field = html.find(r#"id="adevice""#).unwrap();
        let network_heading = html.find("Network ports").unwrap();
        let agwport_field = html.find(r#"id="agwport""#).unwrap();

        assert!(audio_heading < adevice_field);
        assert!(adevice_field < network_heading);
        assert!(network_heading < agwport_field);
    }

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

    #[test]
    fn split_label_extracts_trailing_keyword() {
        assert_eq!(
            split_label("Audio device (ADEVICE)"),
            ("Audio device", Some("ADEVICE"))
        );
        assert_eq!(split_label("PTT"), ("PTT", None));
    }

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
}
