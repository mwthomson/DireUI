# Rename "Configs" page heading to "Configurations"

## Purpose

The config-list page (`/`) currently renders `<h1>Configs</h1>`. Change this
heading text to `<h1>Configurations</h1>` for a more readable, less
abbreviated label.

## Scope

In scope:
- `src/views.rs:194` — the `<h1>Configs</h1>` heading rendered by the
  config-list page.

Out of scope (explicitly not changed):
- The persistent header nav link `<a href="/">Configs</a>` at
  `src/views.rs:34` — this is present on every page as a short label back to
  the config manager, and stays as "Configs".
- The test assertion at `src/views.rs:392` that checks for the nav link text
  (`Configs`) — unaffected since the nav link itself doesn't change.

## Change

`src/views.rs:194`:

```diff
- r##"<h1>Configs</h1>
+ r##"<h1>Configurations</h1>
```

## Testing

No existing test asserts on the `<h1>Configs</h1>` text specifically (only
the nav link at line 392 is asserted), so no test changes are required.
Manual verification: load `/` and confirm the heading reads "Configurations".

## Risks

None — single static string change, no logic affected.
