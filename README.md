# ArchiveCraft

An open-source archive manager written in Rust, part of the Craft app suite (egui with the
craft-libs UI, menu, dialog, settings and translation crates). Licensed **MIT OR Apache-2.0**.

- Create and unpack **ZIP, 7z, tar, tar.gz, tar.xz, tar.zst**; split into parts (`name.zip.001`…).
- **AES-256** passwords for ZIP and 7z (7z can also hide file names); optional weak legacy ZIP
  locking for very old tools.
- **RAR: unpack only**, behind the optional `rar` cargo feature (off by default), through the
  Apache-2.0 `rars` crate's reading API. ArchiveCraft never writes RAR.
- Check (test) archives, look inside files without unpacking, remove items, edit the ZIP note,
  convert between formats, search names and contents (also inside archives), speed test.
- **Recovery data** in the open PAR2 format, and Repair that uses it (or saves every readable
  item when there is none).
- Safe unpacking: absolute paths and `..` that would leave the destination are refused, links
  are never created or followed.
- File-manager right-click actions on Linux (Nautilus, Dolphin, Nemo, Thunar, desktop entry),
  installed and removed from Preferences ▸ File manager.

## Build

```sh
cargo run                      # the window
cargo run --features rar       # with RAR unpacking
cargo run -p archivecraft-cli -- --help
cargo test --workspace
```

## Layout

| Path | What |
|---|---|
| `apps/archivecraft` | the window's binary (window setup only) |
| `apps/archivecraft-cli` | the command-line tool |
| `crates/model` | shared types: items, options, formats, progress, questions, problems |
| `crates/formats` | one module per format, split parts, format detection |
| `crates/par2` | PAR2 recovery data: create, verify, repair |
| `crates/engine` | operations and job control |
| `crates/ui-egui` | everything on screen |

## Command line

```sh
archivecraft-cli create backup.7z ~/Documents -p secret --encrypt-names --recovery 5
archivecraft-cli list backup.7z -p secret
archivecraft-cli test backup.7z -p secret
archivecraft-cli extract backup.7z -d out --overwrite rename -p secret
archivecraft-cli convert old.zip new.tar.zst
archivecraft-cli repair damaged.zip
```

## Right-click menu in the file manager (Linux)

Right-clicking files or folders shows an **ArchiveCraft** submenu:

- Pack into an archive… (opens the Add window)
- Pack into a ZIP right away (`<name>.zip`, default settings)
- Unpack here / Unpack into its own folder (archives only)

Supported: **Dolphin** (KDE service menu), **Files/Nautilus** (a real submenu through
nautilus-python; plain Scripts otherwise), **Nemo** actions, **Thunar** custom actions, and the
app's desktop entry. ArchiveCraft asks once on its first start; it can also be added or taken
out under **Settings ▸ Add ArchiveCraft to my file manager's right-click menu**, under
**Preferences ▸ File manager**, or from a terminal:

```sh
archivecraft-cli integrate            # add it for the file managers found on this computer
archivecraft-cli integrate --status
archivecraft-cli integrate --remove
```

Everything goes into your own folders (`~/.local/share`, `~/.config`), with ArchiveCraft's own
icon. Restart the file manager once if the submenu does not show yet. Windows and macOS menus are
planned and use the same `integration` module.

## Window switches

`archivecraft [paths…]` opens folders or archives. `--add`, `--add-zip`, `--extract-here` and
`--extract-folder` are what the right-click actions call. `--screenshot out.png` draws about
120 frames, saves the window and exits; `--self-test out.json` drives the window with scripted
mouse input and writes what passed.

## Clean room

ArchiveCraft's layout follows a well-known archiver's main window and dialogs, studied only from
screenshots. No code, icons, artwork, colours, fonts or wording were taken from it or any other
archiver; every label is our own and every icon is drawn in code (`crates/ui-egui/src/icons.rs`).
