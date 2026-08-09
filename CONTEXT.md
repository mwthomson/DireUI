# DireUI

A local, web-based config editor for Direwolf, the packet radio TNC software. DireUI makes editing `direwolf.conf` easy and approachable, replacing hand-editing the text file with a form-based UI, while remaining safe to use alongside configs that already have manual edits and comments.

## Language

**Config File**:
The `direwolf.conf` file DireUI is currently pointed at and editing. It is the sole source of truth for Direwolf's configuration — DireUI has no separate internal representation of "the config" beyond this file. Not to be confused with Profile, which is DireUI's own record of a Config File's path and a display name, never a copy of its contents.

**Active Config**:
Which Config File DireUI is currently reading from and writing to. Exactly one Config File is active at a time. Selecting a Profile as active makes that Profile's Config File the Active Config.

**Profile**:
A Config File path DireUI remembers from prior use, plus a DireUI-owned display name, letting a user switch which Config File is Active without re-entering the path or relying on the path itself as the label. The Config File remains the sole source of truth for directive values — a Profile owns only a path and a name, never a copy of the file's contents.
_Avoid_: Known Config, Saved profile

**Curated Directive**:
A Direwolf config directive DireUI understands and exposes through a dedicated form field, with DireUI-side validation.
_Avoid_: Supported directive

**Raw Directive**:
Any line in the Config File DireUI does not model as a Curated Directive — including comments and directives Direwolf added after DireUI last learned about it. Round-trip parsing preserves Raw Directives untouched across saves, and they remain editable only as raw text.
_Avoid_: Unknown directive, unsupported directive

**Backup Preference**:
A per-installation, user-toggleable setting controlling whether DireUI writes a backup of the Config File immediately before each save. Backing up is not a fixed behavior — whether it happens at all is entirely the user's choice.

**Backup History**:
The set of backup copies DireUI has written for a Profile over time, one per save while the Backup Preference is enabled. Unbounded — DireUI does not automatically prune old versions; the user removes individual versions manually. Stored in DireUI's own state directory, not alongside the Config File.
