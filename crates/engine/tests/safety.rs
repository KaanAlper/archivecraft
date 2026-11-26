//! Extraction never writes outside the destination: absolute paths and escaping `..` are refused,
//! links are never created or followed.

use archivecraft_engine::model::{Ctl, ExtractOptions, Overwrite, Severity};
use archivecraft_engine::ops;
use std::fs;
use std::io::Write;
use std::path::Path;

fn zip_with(path: &Path, names: &[&str]) {
    let mut w = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for n in names {
        w.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(b"payload").unwrap();
    }
    w.finish().unwrap();
}

fn extract(archive: &Path, dest: &Path) -> (ops::ExtractReport, Ctl) {
    let ctl = Ctl::default();
    let mut o = ExtractOptions::to(dest);
    o.overwrite = Overwrite::Always;
    let r = ops::extract(archive, &o, None, &ctl).unwrap();
    (r, ctl)
}

#[test]
fn escaping_and_absolute_names_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("evil.zip");
    zip_with(&archive, &["../outside.txt", "a/../../outside2.txt", "/rooted.txt", "C:/drive.txt", "\\\\host\\share.txt", "ok/../fine.txt", "good/file.txt"]);
    let dest = dir.path().join("dest");
    let (r, ctl) = extract(&archive, &dest);
    assert_eq!(r.files, 2, "{r:?}");
    assert_eq!(r.refused, 5, "{r:?}");
    assert!(dest.join("fine.txt").exists());
    assert!(dest.join("good/file.txt").exists());
    assert!(!dir.path().join("outside.txt").exists());
    assert!(!dir.path().join("outside2.txt").exists());
    assert!(!dest.join("rooted.txt").exists());
    let diags = ctl.diags.lock().unwrap();
    assert_eq!(diags.iter().filter(|d| d.severity == Severity::Error).count(), 5);
}

#[cfg(unix)]
#[test]
fn links_in_the_destination_are_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    let outside = dir.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    let dest = dir.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    // A link inside the destination that points outside it.
    std::os::unix::fs::symlink(&outside, dest.join("door")).unwrap();
    // A link to a file outside, with a name the archive also uses.
    fs::write(outside.join("target.txt"), "keep me").unwrap();
    std::os::unix::fs::symlink(outside.join("target.txt"), dest.join("same.txt")).unwrap();

    let archive = dir.path().join("a.zip");
    zip_with(&archive, &["door/planted.txt", "same.txt"]);
    let (r, _) = extract(&archive, &dest);
    assert!(!outside.join("planted.txt").exists());
    assert_eq!(r.refused, 1);
    // The link itself was replaced by a real file; what it pointed to is untouched.
    assert_eq!(fs::read_to_string(outside.join("target.txt")).unwrap(), "keep me");
    assert!(!fs::symlink_metadata(dest.join("same.txt")).unwrap().file_type().is_symlink());
    assert_eq!(fs::read_to_string(dest.join("same.txt")).unwrap(), "payload");
}

#[cfg(unix)]
#[test]
fn links_inside_archives_are_not_created() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("links.tar");
    {
        let mut b = tar::Builder::new(fs::File::create(&archive).unwrap());
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::Symlink);
        h.set_size(0);
        h.set_mode(0o777);
        b.append_link(&mut h, "escape", dir.path()).unwrap();
        let mut h = tar::Header::new_gnu();
        h.set_size(7);
        h.set_mode(0o644);
        b.append_data(&mut h, "escape/through.txt", &b"payload"[..]).unwrap();
        b.finish().unwrap();
    }
    let dest = dir.path().join("dest");
    let (r, _) = extract(&archive, &dest);
    assert!(!fs::symlink_metadata(dest.join("escape")).unwrap().file_type().is_symlink());
    assert!(!dir.path().join("through.txt").exists());
    assert_eq!(r.skipped, 1);
    assert!(dest.join("escape/through.txt").exists());
}
