use std::path::Path;

use crate::state::AppState;

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn page(body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>DireUI</title>
<link rel="stylesheet" href="/style.css">
<script src="/vendor/htmx/htmx.min.js"></script>
</head>
<body>
<header class="site-header"><p class="wordmark">DireUI</p></header>
<main class="page">
{body}
</main>
</body>
</html>
"#
    )
}

fn add_config_form(value: &str, placeholder: &str, label: &str) -> String {
    format!(
        r#"<form class="form-inline" method="post" action="/configs">
<input type="text" name="path" value="{}" placeholder="{}">
<button type="submit">{}</button>
</form>"#,
        html_escape(value),
        html_escape(placeholder),
        html_escape(label)
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
        add_config_form(&suggestion, "/home/user/.direwolf.conf", "Use this config")
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

pub fn config_manager(state: &AppState) -> String {
    let active = state
        .active_config
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "none".to_string());

    let list_items: String = state
        .known_configs
        .iter()
        .map(|p| {
            let display = html_escape(&p.display().to_string());
            if state.active_config.as_deref() == Some(p.as_path()) {
                format!(
                    r#"<li><span class="config-path">{display}</span><span class="config-badge">active</span></li>"#
                )
            } else {
                format!(
                    r#"<li><span class="config-path">{display}</span><form method="post" action="/configs/active"><input type="hidden" name="path" value="{display}"><button type="submit">Switch to this</button></form></li>"#
                )
            }
        })
        .collect();

    format!(
        r#"<h1>Configs</h1>
<p class="meta">Active config: <span class="config-path">{}</span></p>
<ul class="config-list">{}</ul>
{}
{}
<nav class="actions">
<a href="/directives">Edit directives</a>
<a href="/raw">Edit raw config</a>
<button hx-get="/status" hx-swap="outerHTML">Check server status</button>
</nav>"#,
        html_escape(&active),
        list_items,
        add_config_form("", "/home/user/aprs.conf", "Add config"),
        backup_preference_toggle(state.backup_preference)
    )
}

pub fn raw_editor(path: &Path, content: &str) -> String {
    format!(
        r#"<h1>Edit raw config</h1>
<p class="meta">Editing: <span class="config-path">{}</span></p>
<form method="post" action="/raw">
<textarea class="raw-editor" name="content">{}</textarea>
<button type="submit">Save</button>
</form>
<a class="back-link" href="/">Back</a>"#,
        html_escape(&path.display().to_string()),
        html_escape(content)
    )
}

pub struct DirectiveField {
    pub label: &'static str,
    pub name: &'static str,
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

pub fn directives_editor(fields: &[DirectiveField]) -> String {
    let fields_html: String = fields
        .iter()
        .map(|f| {
            let (label_text, keyword) = split_label(f.label);
            let badge_html = keyword
                .map(|k| format!(r#"<span class="field-badge">{}</span>"#, html_escape(k)))
                .unwrap_or_default();
            let error_html = f
                .error
                .map(|msg| format!(r#"<p class="error-text">{}</p>"#, html_escape(msg)))
                .unwrap_or_default();
            format!(
                r#"<div class="field">
<label class="field-label" for="{name}">{label}{badge}</label>
<input type="text" id="{name}" name="{name}" value="{value}">
{error}
</div>"#,
                name = f.name,
                label = html_escape(label_text),
                badge = badge_html,
                value = html_escape(&f.value),
                error = error_html
            )
        })
        .collect();

    format!(
        r#"<h1>Edit directives</h1>
<form method="post" action="/directives">
<div class="panel">
{}
</div>
<button type="submit">Save</button>
</form>
<a class="back-link" href="/">Back</a>"#,
        fields_html
    )
}

pub fn no_active_config() -> String {
    r#"<h1>No active config</h1>
<p>Add a config file before editing.</p>
<a class="back-link" href="/">Back</a>"#
        .to_string()
}

pub fn error(message: &str) -> String {
    format!(
        r#"<h1>Error</h1>
<p class="error-text">{}</p>
<a class="back-link" href="/">Back</a>"#,
        html_escape(message)
    )
}
