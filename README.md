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
