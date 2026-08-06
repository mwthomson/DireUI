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

async fn edit_raw_config(State(ctx): State<AppContext>) -> Html<String> {
    let Some(path) = active_config_path(&ctx) else {
        return Html(views::page(&views::no_active_config()));
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => Html(views::page(&views::raw_editor(&path, &content))),
        Err(err) => Html(views::page(&views::error(&format!(
            "Could not read {}: {err}",
            path.display()
        )))),
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
    if let Err(err) = std::fs::write(&path, content) {
        eprintln!("error: failed to save {}: {err}", path.display());
    }
    Redirect::to("/raw")
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
