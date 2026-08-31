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
    extract::{Query, State},
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

fn write_config(ctx: &AppContext, path: &std::path::Path, content: String) -> Result<(), String> {
    let backup_preference = backup_preference(ctx);
    backup::write_with_backup(path, &content, backup_preference).map_err(|err| {
        eprintln!("error: failed to save {}: {err}", path.display());
        err.to_string()
    })
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
            &views::raw_editor(&path, &content, None),
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
    let current_content = current_content.replace("\r\n", "\n");

    if submitted == current_content {
        return Redirect::to(&format!("/?{}", flash::Flash::NoChange.to_query_string())).into_response();
    }

    match write_config(&ctx, &path, submitted.clone()) {
        Ok(()) => Redirect::to(&format!("/?{}", flash::Flash::Saved.to_query_string())).into_response(),
        Err(err) => Html(views::page(&views::raw_editor(&path, &submitted, Some(&err)), Some(&path)))
            .into_response(),
    }
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
            Html(views::page(&views::directives_editor(&fields, None), Some(&path)))
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
        // Clear's failure UX is out of scope for this change — same
        // best-effort, log-and-continue behavior as before write_config
        // started returning a Result.
        let _ = write_config(&ctx, &path, doc.serialize());
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

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|err| {
            eprintln!("error: failed to bind {addr}: {err}");
            std::process::exit(1);
        });

    println!("DireUI listening on http://{addr}");
    axum::serve(listener, app).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config_path(test_name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("direui-test-{test_name}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("direwolf.conf")
    }

    fn test_ctx(active_config: PathBuf) -> AppContext {
        let mut state = state::AppState::default();
        state.active_config = Some(active_config);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let state_path = std::env::temp_dir().join(format!("direui-test-state-{nanos}.json"));
        AppContext {
            state: Arc::new(Mutex::new(state)),
            store: Arc::new(StateStore::new(state_path)),
            home: None,
        }
    }

    // Regression test for the CRLF normalization asymmetry: an on-disk
    // config with CRLF line endings, saved back unedited (the browser
    // round-trips textarea content as CRLF), must be detected as a
    // no-change save rather than silently rewriting the file's line
    // endings on every save.
    #[tokio::test]
    async fn save_raw_config_detects_no_change_against_crlf_file_on_disk() {
        let path = temp_config_path("crlf-no-change");
        let crlf_content = "CHANNEL 0\r\nMODEM 1200\r\n";
        std::fs::write(&path, crlf_content).unwrap();
        let ctx = test_ctx(path.clone());

        let form = RawConfigForm {
            content: crlf_content.to_string(),
        };

        let response = save_raw_config(State(ctx), Form(form)).await;

        let location = response
            .headers()
            .get(axum::http::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(
            location.contains("flash=nochange"),
            "expected a no-change flash redirect, got {location:?}"
        );

        // The file's original CRLF line endings must be left untouched.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), crlf_content);
    }
}
