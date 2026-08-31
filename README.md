# DireUI
UI for Direwolf configuration and monitoring

## Running

```
cargo run
```

By default DireUI binds to `127.0.0.1:8080` — reachable only from the machine it's running on.

## Network access

DireUI has no authentication. Binding it to anything other than loopback exposes a service that can read and write your Direwolf configuration to everyone who can reach that address — only do this on a network you trust (e.g. a home or club LAN).

To reach DireUI from another machine (for example, a headless Raspberry Pi running Direwolf at a radio site), bind it to an address reachable from your LAN, either via the `--bind` flag:

```
cargo run -- --bind 0.0.0.0:8080
```

or the `DIREUI_BIND` environment variable:

```
DIREUI_BIND=0.0.0.0:8080 cargo run
```

`--bind` takes precedence over `DIREUI_BIND` if both are set.

## Stack

- **Rust** — compiles to a single static binary, no runtime to install (see [ADR 0002](docs/adr/0002-rust-htmx-stack.md)).
- **[axum](https://github.com/tokio-rs/axum)** on **[tokio](https://tokio.rs)** — HTTP server.
- **[serde](https://serde.rs)** — config (de)serialization.
- Server-rendered HTML built in `src/views.rs` — no template engine, no frontend build step.
- **[htmx](https://htmx.org) 4.0.0** — the only client-side JS, vendored in `assets/vendor/htmx/`. Drives partial page updates over HTML.
- Plain CSS in `assets/style.css`.

All assets are embedded into the binary at compile time, so deployment is copy-and-run.
