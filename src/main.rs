mod bind_config;

use axum::{
    Router,
    http::header,
    response::{Html, IntoResponse},
    routing::get,
};

const HTMX_JS: &[u8] = include_bytes!("../assets/vendor/htmx/htmx.min.js");

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>DireUI</title>
<script src="/vendor/htmx/htmx.min.js"></script>
</head>
<body>
<h1>DireUI</h1>
<p>Direwolf configuration is on its way.</p>
<button hx-get="/status" hx-swap="outerHTML">Check server status</button>
</body>
</html>
"#;

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn htmx_js() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], HTMX_JS)
}

async fn status() -> Html<&'static str> {
    Html("<p>DireUI server is running.</p>")
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

    let app = Router::new()
        .route("/", get(index))
        .route("/status", get(status))
        .route("/vendor/htmx/htmx.min.js", get(htmx_js));

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|err| {
            eprintln!("error: failed to bind {addr}: {err}");
            std::process::exit(1);
        });

    println!("DireUI listening on http://{addr}");
    axum::serve(listener, app).await.unwrap();
}
