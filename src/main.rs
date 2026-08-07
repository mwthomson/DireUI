mod bind_config;
mod config;
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
    Html(views::page(&body))
}

async fn htmx_js() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], HTMX_JS)
}

async fn status() -> Html<&'static str> {
    Html("<p>DireUI server is running.</p>")
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

fn active_config_path(ctx: &AppContext) -> Option<PathBuf> {
    ctx.state.lock().unwrap().active_config.clone()
}

fn write_config(path: &std::path::Path, content: String) {
    if let Err(err) = std::fs::write(path, content) {
        eprintln!("error: failed to save {}: {err}", path.display());
    }
}

/// Reads the Active Config's content, or a rendered error page explaining why not.
fn read_active_config(ctx: &AppContext) -> Result<(PathBuf, String), Html<String>> {
    let Some(path) = active_config_path(ctx) else {
        return Err(Html(views::page(&views::no_active_config())));
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok((path, content)),
        Err(err) => Err(Html(views::page(&views::error(&format!(
            "Could not read {}: {err}",
            path.display()
        ))))),
    }
}

async fn edit_raw_config(State(ctx): State<AppContext>) -> Html<String> {
    match read_active_config(&ctx) {
        Ok((path, content)) => Html(views::page(&views::raw_editor(&path, &content))),
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
    write_config(&path, content);
    Redirect::to("/raw")
}

struct FormFieldSpec {
    label: &'static str,
    name: &'static str,
    directive: config::CuratedDirective,
}

// The single source of truth for which curated directives the /directives
// form shows and how form field names map to them. edit_directives and
// save_directives both read from this rather than each enumerating the
// fields themselves.
const CURATED_FIELDS: &[FormFieldSpec] = &[
    FormFieldSpec {
        label: "Audio device (ADEVICE)",
        name: "adevice",
        directive: config::CuratedDirective::AudioDevice,
    },
    FormFieldSpec {
        label: "Channel (CHANNEL)",
        name: "channel",
        directive: config::CuratedDirective::Channel,
    },
    FormFieldSpec {
        label: "Modem (MODEM)",
        name: "modem",
        directive: config::CuratedDirective::Modem,
    },
    FormFieldSpec {
        label: "PTT",
        name: "ptt",
        directive: config::CuratedDirective::Ptt,
    },
];

async fn edit_directives(State(ctx): State<AppContext>) -> Html<String> {
    match read_active_config(&ctx) {
        Ok((_, content)) => {
            let doc = config::Document::parse(&content);
            let fields: Vec<views::DirectiveField> = CURATED_FIELDS
                .iter()
                .map(|spec| views::DirectiveField {
                    label: spec.label,
                    name: spec.name,
                    value: doc.get_curated(spec.directive).unwrap_or("").to_string(),
                    error: None,
                })
                .collect();
            Html(views::page(&views::directives_editor(&fields)))
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
            Ok(()) => fields.push(views::DirectiveField {
                label: spec.label,
                name: spec.name,
                value,
                error: None,
            }),
            Err(err) => {
                any_error = true;
                fields.push(views::DirectiveField {
                    label: spec.label,
                    name: spec.name,
                    value,
                    error: Some(err.message()),
                });
            }
        }
    }

    if any_error {
        return Html(views::page(&views::directives_editor(&fields))).into_response();
    }

    write_config(&path, doc.serialize());
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
        .route("/raw", get(edit_raw_config).post(save_raw_config))
        .route("/directives", get(edit_directives).post(save_directives))
        .route("/status", get(status))
        .route("/vendor/htmx/htmx.min.js", get(htmx_js))
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
