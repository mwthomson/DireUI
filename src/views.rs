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
<title>DireUI</title>
<script src="/vendor/htmx/htmx.min.js"></script>
</head>
<body>
{body}
</body>
</html>
"#
    )
}

fn add_config_form(value: &str, placeholder: &str, label: &str) -> String {
    format!(
        r#"<form method="post" action="/configs">
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
        r#"<p>Backup before save: {status} <form method="post" action="/backup-preference" style="display:inline"><input type="hidden" name="enabled" value="{next_value}"><button type="submit">{button_label}</button></form></p>"#
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
                format!("<li>{display} (active)</li>")
            } else {
                format!(
                    r#"<li>{display} <form method="post" action="/configs/active" style="display:inline"><input type="hidden" name="path" value="{display}"><button type="submit">Switch to this</button></form></li>"#
                )
            }
        })
        .collect();

    format!(
        r#"<h1>DireUI</h1>
<p>Active config: {}</p>
<ul>{}</ul>
{}
{}
<p><a href="/directives">Edit directives</a></p>
<p><a href="/raw">Edit raw config</a></p>
<button hx-get="/status" hx-swap="outerHTML">Check server status</button>"#,
        html_escape(&active),
        list_items,
        add_config_form("", "/home/user/aprs.conf", "Add config"),
        backup_preference_toggle(state.backup_preference)
    )
}

pub fn raw_editor(path: &Path, content: &str) -> String {
    format!(
        r#"<h1>Edit raw config</h1>
<p>Editing: {}</p>
<form method="post" action="/raw">
<textarea name="content" rows="20" cols="80">{}</textarea>
<button type="submit">Save</button>
</form>
<p><a href="/">Back</a></p>"#,
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

pub fn directives_editor(fields: &[DirectiveField]) -> String {
    let fields_html: String = fields
        .iter()
        .map(|f| {
            let error_html = f
                .error
                .map(|msg| format!(r#"<p class="error">{}</p>"#, html_escape(msg)))
                .unwrap_or_default();
            format!(
                r#"<label>{}
<input type="text" name="{}" value="{}">
</label>
{}"#,
                html_escape(f.label),
                f.name,
                html_escape(&f.value),
                error_html
            )
        })
        .collect();

    format!(
        r#"<h1>Edit directives</h1>
<form method="post" action="/directives">
{}
<button type="submit">Save</button>
</form>
<p><a href="/">Back</a></p>"#,
        fields_html
    )
}

pub fn no_active_config() -> String {
    r#"<h1>No active config</h1>
<p>Add a config file before editing.</p>
<p><a href="/">Back</a></p>"#
        .to_string()
}

pub fn error(message: &str) -> String {
    format!(
        r#"<h1>Error</h1>
<p>{}</p>
<p><a href="/">Back</a></p>"#,
        html_escape(message)
    )
}
