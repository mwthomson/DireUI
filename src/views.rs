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
<button hx-get="/status" hx-swap="outerHTML">Check server status</button>"#,
        html_escape(&active),
        list_items,
        add_config_form("", "/home/user/aprs.conf", "Add config")
    )
}
