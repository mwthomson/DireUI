# DireUI

A local, web-based config editor for Direwolf, the packet radio TNC software. DireUI makes editing `direwolf.conf` easy and approachable, replacing hand-editing the text file with a form-based UI, while remaining safe to use alongside configs that already have manual edits and comments.

## Language

**Config File**:
The `direwolf.conf` file DireUI is currently pointed at and editing. It is the sole source of truth for Direwolf's configuration — DireUI has no separate internal representation of "the config" beyond this file.
_Avoid_: Profile

**Active Config**:
Which Config File DireUI is currently reading from and writing to. Exactly one Config File is active at a time.

**Known Config**:
A Config File path DireUI remembers from prior use, letting a user switch which one is Active without re-entering the path. DireUI does not name, describe, duplicate, or otherwise manage Known Configs beyond remembering their paths — the files themselves remain the only source of truth.
_Avoid_: Profile, Saved profile

**Curated Directive**:
A Direwolf config directive DireUI understands and exposes through a dedicated form field, with DireUI-side validation.
_Avoid_: Supported directive

**Raw Directive**:
Any line in the Config File DireUI does not model as a Curated Directive — including comments and directives Direwolf added after DireUI last learned about it. Round-trip parsing preserves Raw Directives untouched across saves, and they remain editable only as raw text.
_Avoid_: Unknown directive, unsupported directive

**Backup Preference**:
A per-installation, user-toggleable setting controlling whether DireUI writes a backup of the Config File immediately before each save. Backing up is not a fixed behavior — whether it happens at all is entirely the user's choice.
