# Attribution

## Bundled assets

ArchiveCraft bundles **no image, icon, sound or font files of its own**.

| Asset | Origin | Licence |
|---|---|---|
| Toolbar, list and dialog icons | Drawn in code with egui shapes (`crates/ui-egui/src/icons.rs`), designed for ArchiveCraft | MIT OR Apache-2.0 (this project) |
| Drag preview picture (Windows/macOS) | Generated in code (`crates/ui-egui/src/dragout.rs`) | MIT OR Apache-2.0 (this project) |
| Interface fonts | egui's default fonts and the craft-ui font stack, from their crates at build time | Their own licences (OFL / Apache / MIT, see those crates) |

Nothing comes from the target program, its folder, or the reference screenshots; those
screenshots are never copied into this repository.

## Formats and specifications

- PAR2 recovery data is written from the public "Parity Volume Set Specification 2.0".
- The RAR test fixture in `crates/formats/tests/rar_read.rs` is built byte by byte in the test from
  the published RAR 5 format description; no RAR writer is used anywhere.

## Libraries

Archive formats: `zip`, `sevenz-rust2`, `lzma-rust2`, `tar`, `flate2` (Rust backend), `ruzstd`,
optional `rars` (read API only). UI: `eframe`/`egui` (storytold fork, as pinned by craft-libs),
`egui_extras`, craft-libs (`craft-ui`, `craft-menu`, `craft-dialogs`, `craft-settings`,
`craft-i18n`, `craft-theme`, `craft-host`, `craft-window`, `craft-tracked`). Support: `clap`,
`serde`, `chrono`, `image`, `md-5`, `crc32fast`, `dirs`, `tempfile`, `trash`, `drag`
(Windows/macOS). All are MIT, Apache-2.0, BSD or similar permissive licences.
