mod backup;
mod bind_config;
mod config;
mod flash;
mod state;
mod store;
mod views;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use axum::{
    Form, Router,
    extract::State,
    http::header,
    response::{Html, IntoResponse, Redirect},
    routing::get,
};
use serde::Deserialize;

use state::AppState;
use store::StateStore;

const HTMX_JS: &[u8] = include_bytes!("../assets/vendor/htmx/htmx.min.js");
const STYLE_CSS: &[u8] = include_bytes!("../assets/style.css");

#[derive(Clone)]
struct AppContext {
    state: Arc<Mutex<AppState>>,
    store: Arc<StateStore>,
    home: Option<PathBuf>,
}

impl AppContext {
    fn mutate_and_save(&self, f: impl FnOnce(&mut AppState)) {
        let mut state = self.state.lock().unwrap();
        f(&mut state);
        if let Err(err) = self.store.save(&state) {
            eprintln!("error: failed to save state: {err}");
        }
    }
}

async fn index(State(ctx): State<AppContext>) -> Html<String> {
    let state = ctx.state.lock().unwrap();
    let body = if state.needs_first_run() {
        let suggestion = ctx
            .home
            .as_deref()
            .and_then(|home| state::suggest_default_config_path(home, |p| p.exists()));
        views::first_run(suggestion.as_deref())
    } else {
        views::config_manager(&state)
    };
    Html(views::page(&body, state.active_config.as_deref()))
}

async fn htmx_js() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], HTMX_JS)
}

async fn style_css() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css")], STYLE_CSS)
}

async fn status() -> Html<String> {
    Html(views::status_indicator())
}

#[derive(Deserialize)]
struct PathForm {
    path: String,
}

async fn add_config(State(ctx): State<AppContext>, Form(form): Form<PathForm>) -> Redirect {
    let path = PathBuf::from(form.path);
    if let Err(err) = store::ensure_config_file_exists(&path) {
        eprintln!("error: failed to create {}: {err}", path.display());
        return Redirect::to("/");
    }

    ctx.mutate_and_save(|state| state.add_known_config(path));
    Redirect::to("/")
}

async fn set_active_config(State(ctx): State<AppContext>, Form(form): Form<PathForm>) -> Redirect {
    let path = PathBuf::from(form.path);

    ctx.mutate_and_save(|state| {
        if let Err(err) = state.set_active_config(&path) {
            eprintln!("error: {err}");
        }
    });
    Redirect::to("/")
}

#[derive(Deserialize)]
struct BackupPreferenceForm {
    enabled: bool,
}

async fn set_backup_preference(
    State(ctx): State<AppContext>,
    Form(form): Form<BackupPreferenceForm>,
) -> Redirect {
    ctx.mutate_and_save(|state| state.set_backup_preference(form.enabled));
    Redirect::to("/")
}

fn active_config_path(ctx: &AppContext) -> Option<PathBuf> {
    ctx.state.lock().unwrap().active_config.clone()
}

fn backup_preference(ctx: &AppContext) -> bool {
    ctx.state.lock().unwrap().backup_preference
}

fn write_config(ctx: &AppContext, path: &std::path::Path, content: String) {
    let backup_preference = backup_preference(ctx);
    if let Err(err) = backup::write_with_backup(path, &content, backup_preference) {
        eprintln!("error: failed to save {}: {err}", path.display());
    }
}

/// Reads the Active Config's content, or a rendered error page explaining why not.
fn read_active_config(ctx: &AppContext) -> Result<(PathBuf, String), Html<String>> {
    let Some(path) = active_config_path(ctx) else {
        return Err(Html(views::page(&views::no_active_config(), None)));
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok((path, content)),
        Err(err) => Err(Html(views::page(
            &views::error(&format!("Could not read {}: {err}", path.display())),
            Some(&path),
        ))),
    }
}

async fn edit_raw_config(State(ctx): State<AppContext>) -> Html<String> {
    match read_active_config(&ctx) {
        Ok((path, content)) => Html(views::page(
            &views::raw_editor(&path, &content),
            Some(&path),
        )),
        Err(page) => page,
    }
}

#[derive(Deserialize)]
struct RawConfigForm {
    content: String,
}

async fn save_raw_config(
    State(ctx): State<AppContext>,
    Form(form): Form<RawConfigForm>,
) -> Redirect {
    let Some(path) = active_config_path(&ctx) else {
        return Redirect::to("/");
    };
    // Browsers normalize textarea line breaks to CRLF on form submission
    // regardless of the file's original line endings — undo that so an
    // unedited save doesn't rewrite every line ending in the file.
    let content = form.content.replace("\r\n", "\n");
    write_config(&ctx, &path, content);
    Redirect::to("/raw")
}

struct FormFieldSpec {
    label: &'static str,
    name: &'static str,
    group: &'static str,
    multiline: bool,
    directive: config::CuratedDirective,
}

impl FormFieldSpec {
    fn to_directive_field(&self, value: String, error: Option<&'static str>) -> views::DirectiveField {
        views::DirectiveField {
            label: self.label,
            name: self.name,
            group: self.group,
            multiline: self.multiline,
            // CHANNEL scopes the MODEM/PTT lines beneath it (see the NOTE
            // on Document::get_curated) — excluded from Clear since
            // removing it can silently orphan those scoped lines.
            clearable: !matches!(self.directive, config::CuratedDirective::Channel),
            value,
            error,
        }
    }
}

// The single source of truth for which curated directives the /directives
// form shows, how form field names map to them, and which of the five
// directive areas each belongs to for grouping in the UI. edit_directives
// and save_directives both read from this rather than each enumerating the
// fields themselves.
const CURATED_FIELDS: &[FormFieldSpec] = &[
    FormFieldSpec {
        label: "Audio device (ADEVICE)",
        name: "adevice",
        group: "Audio device",
        multiline: false,
        directive: config::CuratedDirective::AudioDevice,
    },
    FormFieldSpec {
        label: "Channel (CHANNEL)",
        name: "channel",
        group: "Channel, modem & PTT",
        multiline: false,
        directive: config::CuratedDirective::Channel,
    },
    FormFieldSpec {
        label: "Modem (MODEM)",
        name: "modem",
        group: "Channel, modem & PTT",
        multiline: false,
        directive: config::CuratedDirective::Modem,
    },
    FormFieldSpec {
        label: "PTT",
        name: "ptt",
        group: "Channel, modem & PTT",
        multiline: false,
        directive: config::CuratedDirective::Ptt,
    },
    FormFieldSpec {
        label: "AGW network port (AGWPORT)",
        name: "agwport",
        group: "Network ports",
        multiline: false,
        directive: config::CuratedDirective::AgwPort,
    },
    FormFieldSpec {
        label: "KISS network port (KISSPORT)",
        name: "kissport",
        group: "Network ports",
        multiline: false,
        directive: config::CuratedDirective::KissPort,
    },
    FormFieldSpec {
        label: "Position beacon (PBEACON)",
        name: "pbeacon",
        group: "APRS beaconing",
        multiline: true,
        directive: config::CuratedDirective::PBeacon,
    },
    FormFieldSpec {
        label: "Custom beacon (CBEACON)",
        name: "cbeacon",
        group: "APRS beaconing",
        multiline: true,
        directive: config::CuratedDirective::CBeacon,
    },
    FormFieldSpec {
        label: "Digipeat (DIGIPEAT)",
        name: "digipeat",
        group: "Digipeating",
        multiline: false,
        directive: config::CuratedDirective::Digipeat,
    },
];

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
            Html(views::page(&views::directives_editor(&fields), Some(&path)))
        }
        Err(page) => page,
    }
}

async fn save_directives(
    State(ctx): State<AppContext>,
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> axum::response::Response {
    let (path, content) = match read_active_config(&ctx) {
        Ok(pair) => pair,
        Err(page) => return page.into_response(),
    };
    let mut doc = config::Document::parse(&content);

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
        return Html(views::page(&views::directives_editor(&fields), Some(&path))).into_response();
    }

    write_config(&ctx, &path, doc.serialize());
    Redirect::to("/directives").into_response()
}

#[derive(Deserialize)]
struct ClearDirectiveForm {
    clear_field: String,
}

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
        write_config(&ctx, &path, doc.serialize());
    }

    Redirect::to("/directives").into_response()
}

fn cli_bind_arg(args: &[String]) -> Option<String> {
    args.iter()
        .position(|a| a == "--bind")
        .and_then(|i| args.get(i + 1))
        .cloned()
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cli_arg = cli_bind_arg(&args);
    let env_var = std::env::var("DIREUI_BIND").ok();

    let addr = bind_config::resolve_bind_address(cli_arg.as_deref(), env_var.as_deref())
        .unwrap_or_else(|err| {
            eprintln!("error: {err}");
            std::process::exit(1);
        });

    let home = std::env::var("HOME").ok().map(PathBuf::from);
    let xdg_config_home = std::env::var("XDG_CONFIG_HOME").ok();
    let state_path =
        store::resolve_state_path(xdg_config_home.as_deref(), home.as_deref().and_then(|p| p.to_str()))
            .unwrap_or_else(|| {
                eprintln!("error: could not determine a config directory (HOME is not set)");
                std::process::exit(1);
            });

    let store = StateStore::new(state_path);
    let state = store.load();

    let ctx = AppContext {
        state: Arc::new(Mutex::new(state)),
        store: Arc::new(store),
        home,
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/configs", axum::routing::post(add_config))
        .route("/configs/active", axum::routing::post(set_active_config))
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

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|err| {
            eprintln!("error: failed to bind {addr}: {err}");
            std::process::exit(1);
        });

    println!("DireUI listening on http://{addr}");
    axum::serve(listener, app).await.unwrap();
}
