//! Round trips: create with each writable format, list, test, extract, compare byte for byte.

use archivecraft_engine::model::{Answer, CreateOptions, Ctl, ExtractOptions, Format, Overwrite, UpdateMode};
use archivecraft_engine::{formats, ops, Error};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// A small tree with text, binary, empty, already-compressed files and an empty folder.
fn make_tree(root: &Path) -> PathBuf {
    let p = root.join("project");
    fs::create_dir_all(p.join("docs/sub")).unwrap();
    fs::create_dir_all(p.join("emptydir")).unwrap();
    fs::write(p.join("docs/readme.txt"), "Hello from ArchiveCraft.\n".repeat(200)).unwrap();
    // Pseudo-random bytes that do not compress.
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let bin: Vec<u8> = (0..150_000)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 32) as u8
        })
        .collect();
    fs::write(p.join("docs/sub/deep.bin"), bin).unwrap();
    fs::write(p.join("empty.txt"), "").unwrap();
    fs::write(p.join("photo.jpg"), vec![0xFFu8; 5000]).unwrap();
    p
}

fn files_under(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            if p.is_dir() {
                out.push((rel + "/", Vec::new()));
                stack.push(p);
            } else {
                out.push((rel, fs::read(&p).unwrap()));
            }
        }
    }
    out.sort();
    out
}

fn extract_all(archive: &Path, dest: &Path, password: Option<&str>) -> Result<ops::ExtractReport, Error> {
    let mut o = ExtractOptions::to(dest);
    o.overwrite = Overwrite::Always;
    ops::extract(archive, &o, password, &Ctl::default())
}

fn round_trip(format: Format, password: Option<&str>, level: u32) {
    let dir = tempfile::tempdir().unwrap();
    let src = make_tree(dir.path());
    let archive = dir.path().join(format!("out.{}", format.extension()));
    let opts = CreateOptions { format, level, password: password.map(str::to_string), ..CreateOptions::default() };
    let rep = ops::create(std::slice::from_ref(&src), &archive, &opts, &Ctl::default()).unwrap();
    assert_eq!(rep.added, 4, "{format:?}");

    let listing = formats::list(&archive, password).unwrap();
    assert_eq!(listing.format, format);
    let names: Vec<&str> = listing.entries.iter().map(|e| e.path.as_str()).collect();
    for n in ["project/docs/readme.txt", "project/docs/sub/deep.bin", "project/empty.txt", "project/photo.jpg", "project/emptydir"] {
        assert!(names.contains(&n), "{format:?} lacks {n}: {names:?}");
    }
    assert_eq!(listing.any_encrypted(), password.is_some(), "{format:?}");

    let t = ops::test(&archive, password, &Ctl::default()).unwrap();
    assert_eq!(t.errors, 0, "{format:?}");
    assert_eq!(t.files, 4);

    let out = dir.path().join("unpacked");
    let r = extract_all(&archive, &out, password).unwrap();
    assert_eq!(r.files, 4, "{format:?}");
    assert_eq!(files_under(&out.join("project")), files_under(&src), "{format:?}");
    // Modification times survive (ZIP keeps two-second steps).
    let a = fs::metadata(src.join("docs/readme.txt")).unwrap().modified().unwrap();
    let b = fs::metadata(out.join("project/docs/readme.txt")).unwrap().modified().unwrap();
    let diff = a.duration_since(b).or_else(|_| b.duration_since(a)).unwrap();
    assert!(diff <= Duration::from_secs(2), "{format:?}: {diff:?}");
}

#[test]
fn zip_round_trip() {
    round_trip(Format::Zip, None, 5);
    round_trip(Format::Zip, None, 0);
}

#[test]
fn sevenz_round_trip() {
    round_trip(Format::SevenZ, None, 5);
    round_trip(Format::SevenZ, None, 0);
}

#[test]
fn tar_round_trip() {
    round_trip(Format::Tar, None, 0);
}

#[test]
fn tar_gz_round_trip() {
    round_trip(Format::TarGz, None, 6);
}

#[test]
fn tar_xz_round_trip() {
    round_trip(Format::TarXz, None, 3);
}

#[test]
fn tar_zst_round_trip() {
    round_trip(Format::TarZst, None, 3);
    round_trip(Format::TarZst, None, 0);
}

#[test]
fn zip_aes256_round_trip() {
    round_trip(Format::Zip, Some("correct horse"), 5);
}

#[test]
fn sevenz_aes256_round_trip() {
    round_trip(Format::SevenZ, Some("correct horse"), 5);
}

#[test]
fn passwords_are_checked() {
    for format in [Format::Zip, Format::SevenZ] {
        let dir = tempfile::tempdir().unwrap();
        let src = make_tree(dir.path());
        let archive = dir.path().join(format!("locked.{}", format.extension()));
        let opts = CreateOptions { format, password: Some("right".into()), ..CreateOptions::default() };
        ops::create(&[src], &archive, &opts, &Ctl::default()).unwrap();
        let out = dir.path().join("o");
        assert!(matches!(extract_all(&archive, &out, None), Err(Error::PasswordRequired)), "{format:?}");
        assert!(matches!(extract_all(&archive, &out, Some("wrong")), Err(Error::WrongPassword)), "{format:?}");
        // With someone to ask, the right answer gets through.
        let ctl = Ctl::with_asker(Box::new(|_| Answer::Password("right".into())));
        let mut o = ExtractOptions::to(&out);
        o.overwrite = Overwrite::Always;
        let r = ops::extract(&archive, &o, None, &ctl).unwrap();
        assert_eq!(r.files, 4);
        assert_eq!(r.password_used.as_deref(), Some("right"));
    }
}

#[test]
fn sevenz_locked_names() {
    let dir = tempfile::tempdir().unwrap();
    let src = make_tree(dir.path());
    let archive = dir.path().join("names.7z");
    let opts = CreateOptions { format: Format::SevenZ, password: Some("pw".into()), encrypt_names: true, ..CreateOptions::default() };
    ops::create(&[src], &archive, &opts, &Ctl::default()).unwrap();
    assert!(formats::list(&archive, None).unwrap_err().is_password());
    let l = formats::list(&archive, Some("pw")).unwrap();
    assert!(l.names_encrypted);
    assert_eq!(l.file_count(), 4);
}

#[test]
fn zip_legacy_encryption() {
    let dir = tempfile::tempdir().unwrap();
    let src = make_tree(dir.path());
    let archive = dir.path().join("old.zip");
    let opts = CreateOptions { format: Format::Zip, password: Some("pw".into()), zip_legacy_crypto: true, ..CreateOptions::default() };
    ops::create(std::slice::from_ref(&src), &archive, &opts, &Ctl::default()).unwrap();
    let out = dir.path().join("o");
    extract_all(&archive, &out, Some("pw")).unwrap();
    assert_eq!(files_under(&out.join("project")), files_under(&src));
}

#[test]
fn split_volumes_round_trip() {
    for format in [Format::Zip, Format::SevenZ, Format::TarGz, Format::TarXz, Format::TarZst] {
        let dir = tempfile::tempdir().unwrap();
        let src = make_tree(dir.path());
        let archive = dir.path().join(format!("split.{}", format.extension()));
        let opts = CreateOptions { format, level: 1, volume_size: Some(6_000), store_patterns: Vec::new(), ..CreateOptions::default() };
        let rep = ops::create(std::slice::from_ref(&src), &archive, &opts, &Ctl::default()).unwrap();
        assert!(rep.parts.len() > 1, "{format:?}: {:?}", rep.parts);
        assert!(rep.parts[0].to_string_lossy().ends_with(".001"));
        let out = dir.path().join("o");
        extract_all(&rep.parts[0], &out, None).unwrap();
        assert_eq!(files_under(&out.join("project")), files_under(&src), "{format:?}");
    }
}

#[test]
fn solid_7z_and_recovery_data() {
    let dir = tempfile::tempdir().unwrap();
    let src = make_tree(dir.path());
    let archive = dir.path().join("solid.7z");
    let opts = CreateOptions { format: Format::SevenZ, solid: true, recovery_percent: 10, test_after: true, ..CreateOptions::default() };
    let rep = ops::create(std::slice::from_ref(&src), &archive, &opts, &Ctl::default()).unwrap();
    assert_eq!(rep.test_errors, 0);
    assert_eq!(rep.recovery_files.len(), 2);
    assert!(formats::list(&archive, None).unwrap().solid);
    // Damage the archive, then repair it from the recovery data.
    let good = fs::read(&archive).unwrap();
    let mut bad = good.clone();
    let mid = bad.len() / 2;
    for b in &mut bad[mid..mid + 64] {
        *b ^= 0xFF;
    }
    fs::write(&archive, &bad).unwrap();
    let ctl = Ctl::default();
    let r = archivecraft_engine::repair::repair(&archive, dir.path(), None, None, &ctl).unwrap();
    assert_eq!(r.method, archivecraft_engine::repair::RepairMethod::RecoveryData);
    assert_eq!(fs::read(&archive).unwrap(), good);
}

#[test]
fn update_modes() {
    let dir = tempfile::tempdir().unwrap();
    let src = make_tree(dir.path());
    let archive = dir.path().join("u.zip");
    let ctl = Ctl::default();
    ops::create(std::slice::from_ref(&src), &archive, &CreateOptions::default(), &ctl).unwrap();

    // Change one file (newer), add one, remove one.
    let later = SystemTime::now() + Duration::from_secs(60);
    fs::write(src.join("docs/readme.txt"), "changed").unwrap();
    fs::File::options().write(true).open(src.join("docs/readme.txt")).unwrap().set_modified(later).unwrap();
    fs::write(src.join("new.txt"), "new").unwrap();
    fs::remove_file(src.join("photo.jpg")).unwrap();

    // Freshen: only the changed file; the new one stays out.
    let o = CreateOptions { update: UpdateMode::Freshen, ..CreateOptions::default() };
    ops::create(std::slice::from_ref(&src), &archive, &o, &ctl).unwrap();
    let l = formats::list(&archive, None).unwrap();
    assert!(!l.entries.iter().any(|e| e.path == "project/new.txt"));
    assert_eq!(formats::read_one(&archive, "project/docs/readme.txt", None, 1000).unwrap(), b"changed");

    // Mirror: the new file comes in, the removed one goes.
    let o = CreateOptions { update: UpdateMode::Sync, ..CreateOptions::default() };
    let rep = ops::create(std::slice::from_ref(&src), &archive, &o, &ctl).unwrap();
    assert_eq!(rep.removed, 1);
    let l = formats::list(&archive, None).unwrap();
    assert!(l.entries.iter().any(|e| e.path == "project/new.txt"));
    assert!(!l.entries.iter().any(|e| e.path == "project/photo.jpg"));

    // Leave alone: an existing name is not replaced.
    fs::write(src.join("new.txt"), "newer content").unwrap();
    let o = CreateOptions { update: UpdateMode::SkipExisting, ..CreateOptions::default() };
    ops::create(std::slice::from_ref(&src), &archive, &o, &ctl).unwrap();
    assert_eq!(formats::read_one(&archive, "project/new.txt", None, 1000).unwrap(), b"new");
}

#[test]
fn delete_comment_convert() {
    let dir = tempfile::tempdir().unwrap();
    let src = make_tree(dir.path());
    let archive = dir.path().join("d.zip");
    let ctl = Ctl::default();
    ops::create(std::slice::from_ref(&src), &archive, &CreateOptions::default(), &ctl).unwrap();
    let removed = ops::delete(&archive, &["project/docs".into()], None, &ctl).unwrap();
    assert!(removed >= 2);
    let l = formats::list(&archive, None).unwrap();
    assert!(l.entries.iter().all(|e| !e.path.starts_with("project/docs")));
    assert!(l.entries.iter().any(|e| e.path == "project/photo.jpg"));

    ops::set_comment(&archive, "Our own note.", None, &ctl).unwrap();
    assert_eq!(formats::list(&archive, None).unwrap().comment.as_deref(), Some("Our own note."));

    for to in [Format::SevenZ, Format::TarXz] {
        let dst = dir.path().join(format!("c.{}", to.extension()));
        let opts = CreateOptions { format: to, ..CreateOptions::default() };
        ops::convert(&archive, &dst, &opts, None, &ctl).unwrap();
        let out = dir.path().join(format!("conv-{}", to.extension()));
        extract_all(&dst, &out, None).unwrap();
        assert_eq!(fs::read(out.join("project/photo.jpg")).unwrap(), fs::read(src.join("photo.jpg")).unwrap());
    }
}

#[test]
fn overwrite_rules() {
    let dir = tempfile::tempdir().unwrap();
    let src = make_tree(dir.path());
    let archive = dir.path().join("o.7z");
    let ctl = Ctl::default();
    ops::create(std::slice::from_ref(&src), &archive, &CreateOptions { format: Format::SevenZ, ..CreateOptions::default() }, &ctl).unwrap();
    let out = dir.path().join("out");
    extract_all(&archive, &out, None).unwrap();
    let f = out.join("project/docs/readme.txt");
    fs::write(&f, "mine").unwrap();

    let mut o = ExtractOptions::to(&out);
    o.overwrite = Overwrite::Skip;
    let r = ops::extract(&archive, &o, None, &ctl).unwrap();
    assert_eq!(r.files, 0);
    assert_eq!(fs::read_to_string(&f).unwrap(), "mine");

    o.overwrite = Overwrite::Rename;
    ops::extract(&archive, &o, None, &ctl).unwrap();
    assert!(out.join("project/docs/readme (2).txt").exists());
    assert_eq!(fs::read_to_string(&f).unwrap(), "mine");

    // "Ask" with an answer of "replace all".
    let asked = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let a2 = asked.clone();
    let ctl = Ctl::with_asker(Box::new(move |_| {
        a2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Answer::YesToAll
    }));
    o.overwrite = Overwrite::Ask;
    o.only = vec!["project/docs".into()];
    o.base = "project".into();
    // First time: nothing there yet under docs/, so no question.
    let r = ops::extract(&archive, &o, None, &ctl).unwrap();
    assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(r.files, 2);
    // `base` removed the "project/" prefix.
    assert!(out.join("docs/readme.txt").exists());
    // Second time: one question, answered "replace all", covers both files.
    let r = ops::extract(&archive, &o, None, &ctl).unwrap();
    assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(r.files, 2);
}
