//! The operations both the window and the command line run: create, add, extract, test,
//! preview, delete, convert, comment.

use archivecraft_formats::{self as reader, open_sink, volumes, Sink};
use archivecraft_model::util::{matches_any, safe_relative, unique_path};
use archivecraft_model::*;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

/// A file or folder on disk that goes into an archive.
#[derive(Debug, Clone)]
pub struct Input {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub mtime: Option<SystemTime>,
    pub mode: Option<u32>,
}

#[cfg(unix)]
fn mode_of(m: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(m.permissions().mode())
}
#[cfg(not(unix))]
fn mode_of(_: &fs::Metadata) -> Option<u32> {
    None
}

fn display_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.display().to_string())
}

/// Name an item gets inside the archive for a given path mode.
fn archive_name(path: &Path, source_root: &Path, mode: PathMode) -> String {
    match mode {
        PathMode::Relative => {
            let base = source_root.parent().unwrap_or(Path::new(""));
            let rel = path.strip_prefix(base).unwrap_or(path);
            normalize_name(&rel.to_string_lossy())
        }
        PathMode::NoPaths => display_name(path),
        PathMode::Full => {
            let parts: Vec<String> = path
                .components()
                .filter_map(|c| match c {
                    std::path::Component::Normal(s) => Some(s.to_string_lossy().to_string()),
                    _ => None,
                })
                .collect();
            parts.join("/")
        }
    }
}

/// Walk the chosen files and folders and decide what goes in, applying exclusions and the time filter.
pub fn collect_inputs(sources: &[PathBuf], opts: &CreateOptions, ctl: &Ctl) -> Result<Vec<Input>> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for src in sources {
        let src = if src.is_absolute() { src.clone() } else { std::env::current_dir()?.join(src) };
        let mut stack = vec![src.clone()];
        while let Some(p) = stack.pop() {
            ctl.check()?;
            let meta = match fs::symlink_metadata(&p) {
                Ok(m) => m,
                Err(e) => {
                    ctl.warn(&p.display().to_string(), format!("could not read: {e}"));
                    continue;
                }
            };
            let name = archive_name(&p, &src, opts.path_mode);
            if name.is_empty() {
                continue;
            }
            if matches_any(&opts.exclude, &name) {
                continue;
            }
            if meta.file_type().is_symlink() {
                // Follow links to files; never walk into linked folders (avoids loops).
                match fs::metadata(&p) {
                    Ok(m) if m.is_file() => {}
                    _ => {
                        ctl.info(&p.display().to_string(), "linked folder left out");
                        continue;
                    }
                }
            }
            let meta = fs::metadata(&p).unwrap_or(meta);
            if meta.is_dir() {
                if opts.path_mode != PathMode::NoPaths && seen.insert(name.clone()) {
                    out.push(Input { path: p.clone(), name, is_dir: true, size: 0, mtime: meta.modified().ok(), mode: mode_of(&meta) });
                }
                let mut children: Vec<PathBuf> = match fs::read_dir(&p) {
                    Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
                    Err(e) => {
                        ctl.warn(&p.display().to_string(), format!("could not list folder: {e}"));
                        continue;
                    }
                };
                children.sort();
                children.reverse();
                stack.extend(children);
            } else {
                let mtime = meta.modified().ok();
                if !opts.time_filter.accepts(mtime) {
                    continue;
                }
                if !seen.insert(name.clone()) {
                    ctl.warn(&p.display().to_string(), format!("another item is already called \"{name}\"; this one was left out"));
                    continue;
                }
                out.push(Input { path: p.clone(), name, is_dir: false, size: meta.len(), mtime, mode: mode_of(&meta) });
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Default, Clone)]
pub struct CreateReport {
    pub added: usize,
    pub kept: usize,
    pub removed: usize,
    pub parts: Vec<PathBuf>,
    pub recovery_files: Vec<PathBuf>,
    pub test_errors: usize,
}

fn newer(a: Option<SystemTime>, b: Option<SystemTime>) -> bool {
    match (a, b) {
        // ZIP keeps two-second times; allow for that.
        (Some(a), Some(b)) => a.duration_since(b).map(|d| d.as_secs() > 2).unwrap_or(false),
        (Some(_), None) => true,
        _ => false,
    }
}

fn temp_beside(path: &Path) -> PathBuf {
    let name = display_name(path);
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    for i in 0..1000 {
        let p = dir.join(format!(".{name}.acwork{i}"));
        if !p.exists() {
            return p;
        }
    }
    dir.join(format!(".{name}.acwork"))
}

/// Copy the kept items of an existing archive into a sink.
fn copy_existing(src: &Path, password: Option<&str>, keep: &dyn Fn(&Entry) -> bool, sink: &mut dyn Sink, store: &[String], ctl: &Ctl, count: &AtomicU64) -> Result<usize> {
    let mut n = 0;
    let mut want = |e: &Entry| keep(e);
    let label = display_name(src);
    let mut f = |e: &Entry, r: &mut dyn Read| -> Result<()> {
        ctl.check()?;
        ctl.item(&e.path, e.size);
        if e.is_dir {
            sink.add_dir(&e.path, e.modified)?;
        } else if e.is_link {
            ctl.info(&label, format!("link \"{}\" was not carried over", e.path));
        } else {
            let mut pr = ProgressReader { inner: r, ctl };
            sink.add_file(&e.path, e.modified, e.size, e.mode, matches_any(store, &e.path), &mut pr)?;
        }
        ctl.set_out_bytes(count.load(Ordering::Relaxed));
        n += 1;
        Ok(())
    };
    reader::visit(src, password, &mut want, &mut f)?;
    Ok(n)
}

/// Create a new archive from files on disk, or add them to an existing one.
pub fn create(sources: &[PathBuf], archive: &Path, opts: &CreateOptions, ctl: &Ctl) -> Result<CreateReport> {
    let label = display_name(archive);
    ctl.archive(&archive.display().to_string());
    ctl.stage("Looking at the files");
    let inputs = collect_inputs(sources, opts, ctl)?;
    let existing = archive.exists();
    let mut opts = opts.clone();
    let mut report = CreateReport::default();

    let mut old: Option<Listing> = None;
    if existing {
        if volumes::is_first_volume(archive) {
            return Err(Error::Unsupported("a split archive cannot be changed; join it first".into()));
        }
        let fmt = reader::detect(archive)?;
        if !fmt.can_write() {
            return Err(Error::Unsupported(format!("{} archives can only be read", fmt.label())));
        }
        opts.format = fmt;
        old = Some(reader::list(archive, opts.password.as_deref())?);
    } else if !opts.format.can_write() {
        return Err(Error::Unsupported(format!("{} archives can only be read", opts.format.label())));
    }

    // Decide which inputs go in and which old items stay.
    let old_map: HashMap<String, Entry> = old.as_ref().map(|l| l.entries.iter().map(|e| (e.path.clone(), e.clone())).collect()).unwrap_or_default();
    let mut add: Vec<Input> = Vec::new();
    let mut replaced: HashSet<String> = HashSet::new();
    let mut ask_mode = opts.update;
    for inp in &inputs {
        let prev = old_map.get(&inp.name);
        if inp.is_dir {
            if prev.is_none() && opts.update != UpdateMode::Freshen {
                add.push(inp.clone());
            }
            continue;
        }
        let take = match (ask_mode, prev) {
            (_, None) => opts.update != UpdateMode::Freshen,
            (UpdateMode::AddReplace, Some(_)) => true,
            (UpdateMode::Update | UpdateMode::Sync | UpdateMode::Freshen, Some(p)) => newer(inp.mtime, p.modified),
            (UpdateMode::SkipExisting, Some(_)) => false,
            (UpdateMode::Ask, Some(p)) => match ctl.ask(Question::Replace {
                path: inp.name.clone(),
                old_size: Some(p.size),
                old_time: p.modified,
                new_size: inp.size,
                new_time: inp.mtime,
            }) {
                Answer::Yes | Answer::Rename => true,
                Answer::YesToAll | Answer::RenameAll => {
                    ask_mode = UpdateMode::AddReplace;
                    true
                }
                Answer::No | Answer::Password(_) => false,
                Answer::NoToAll => {
                    ask_mode = UpdateMode::SkipExisting;
                    false
                }
                Answer::Cancel => return Err(Error::Cancelled),
            },
        };
        if take {
            if prev.is_some() {
                replaced.insert(inp.name.clone());
            }
            add.push(inp.clone());
        }
    }
    let input_names: HashSet<String> = inputs.iter().map(|i| i.name.clone()).collect();
    let sync = opts.update == UpdateMode::Sync;
    let keep = |e: &Entry| -> bool { !replaced.contains(&e.path) && !(sync && !input_names.contains(&e.path) && !e.is_dir) };
    if let Some(l) = &old {
        report.removed = l.entries.iter().filter(|e| !e.is_dir && sync && !input_names.contains(&e.path)).count();
    }
    if existing && add.iter().all(|i| i.is_dir) && report.removed == 0 && opts.comment.is_none() {
        ctl.info(&label, "nothing needed to change");
        return Ok(report);
    }

    let kept_bytes: u64 = old.as_ref().map(|l| l.entries.iter().filter(|e| keep(e)).map(|e| e.size).sum()).unwrap_or(0);
    let new_bytes: u64 = add.iter().map(|i| i.size).sum();
    ctl.set_totals((add.len() + old.as_ref().map(|l| l.entries.len()).unwrap_or(0)) as u64, kept_bytes + new_bytes);

    let work = if existing { temp_beside(archive) } else { archive.to_path_buf() };
    if let Some(parent) = work.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let count = Arc::new(AtomicU64::new(0));
    let result = (|| -> Result<()> {
        let mut sink = open_sink(&work, &opts, count.clone())?;
        if let Some(l) = &old {
            ctl.stage("Copying items already inside");
            report.kept = copy_existing(archive, opts.password.as_deref(), &keep, sink.as_mut(), &opts.store_patterns, ctl, &count)?;
            if opts.comment.is_none()
                && let Some(c) = &l.comment
            {
                sink.set_comment(c)?;
            }
        }
        ctl.stage("Adding");
        for inp in &add {
            ctl.check()?;
            ctl.item(&inp.name, inp.size);
            let mtime = if opts.store_mtime { inp.mtime } else { Some(SystemTime::now()) };
            if inp.is_dir {
                sink.add_dir(&inp.name, mtime)?;
            } else {
                let store = matches_any(&opts.store_patterns, &inp.name);
                if let Err(e) = sink.add_path(&inp.name, &inp.path, mtime, inp.mode, store, ctl) {
                    if matches!(e, Error::Cancelled) {
                        return Err(e);
                    }
                    // A file that vanished or cannot be read: an archive with a half-written item is
                    // broken for zip/tar, so stop here.
                    return Err(Error::other(format!("could not add {}: {e}", inp.path.display())));
                }
            }
            ctl.set_out_bytes(count.load(Ordering::Relaxed));
            report.added += usize::from(!inp.is_dir);
        }
        if let Some(c) = &opts.comment {
            if opts.format.supports_comment() {
                sink.set_comment(c)?;
            } else if !c.trim().is_empty() {
                ctl.info(&label, format!("{} cannot hold a comment; it was left out", opts.format.label()));
            }
        }
        ctl.stage("Finishing");
        sink.finish()?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&work);
        return Err(e);
    }
    if existing {
        fs::rename(&work, archive)?;
    }

    // Split, test, recovery data, then remove sources.
    report.parts = match opts.volume_size {
        Some(sz) if !existing => {
            ctl.stage("Splitting into parts");
            volumes::split_file(archive, sz)?
        }
        _ => vec![archive.to_path_buf()],
    };
    let first = report.parts[0].clone();
    if opts.test_after {
        let t = test(&first, opts.password.as_deref(), ctl)?;
        report.test_errors = t.errors;
    }
    if opts.recovery_percent > 0 {
        ctl.stage("Writing recovery data");
        report.recovery_files = archivecraft_par2::create(&report.parts, opts.recovery_percent, ctl)?;
    }
    if opts.delete_sources && report.test_errors == 0 && !ctl.is_cancelled() {
        ctl.stage("Removing the original files");
        delete_inputs(&inputs, ctl);
    }
    Ok(report)
}

fn delete_inputs(inputs: &[Input], ctl: &Ctl) {
    for i in inputs.iter().filter(|i| !i.is_dir) {
        if let Err(e) = fs::remove_file(&i.path) {
            ctl.warn(&i.path.display().to_string(), format!("could not remove: {e}"));
        }
    }
    let mut dirs: Vec<&Input> = inputs.iter().filter(|i| i.is_dir).collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.path.components().count()));
    for d in dirs {
        let _ = fs::remove_dir(&d.path); // only if empty
    }
}

// ---------------------------------------------------------------- extract

#[derive(Debug, Default, Clone)]
pub struct ExtractReport {
    pub files: usize,
    pub folders: usize,
    pub skipped: usize,
    pub refused: usize,
    pub errors: usize,
    pub password_used: Option<String>,
}

/// Is any folder between `root` and `root/rel` (not including the last part) a symbolic link?
/// Writing through one could put files outside the destination.
pub fn has_link_ancestor(root: &Path, rel: &Path) -> bool {
    let mut p = root.to_path_buf();
    let comps: Vec<_> = rel.components().collect();
    for c in comps.iter().take(comps.len().saturating_sub(1)) {
        p.push(c);
        if let Ok(m) = fs::symlink_metadata(&p)
            && m.file_type().is_symlink()
        {
            return true;
        }
    }
    false
}

#[cfg(unix)]
fn apply_mode(f: &File, mode: Option<u32>) {
    use std::os::unix::fs::PermissionsExt;
    if let Some(m) = mode {
        // Keep ordinary permission bits only (no set-id or sticky bits).
        let _ = f.set_permissions(fs::Permissions::from_mode((m & 0o777) | 0o200));
    }
}
#[cfg(not(unix))]
fn apply_mode(_: &File, _: Option<u32>) {}

/// Ask for a password when an archive needs one. Returns `None` if the person gave up.
fn ask_password(ctl: &Ctl, archive: &Path, item: &str, wrong: bool) -> Option<String> {
    if !ctl.has_asker() {
        return None;
    }
    match ctl.ask(Question::Password { archive: display_name(archive), item: item.to_string(), wrong }) {
        Answer::Password(p) if !p.is_empty() => Some(p),
        _ => None,
    }
}

/// Run `job` with a password, asking again (up to three times) when it is missing or wrong.
fn with_password<T>(archive: &Path, password: Option<&str>, ctl: &Ctl, first_item: &str, mut job: impl FnMut(Option<&str>) -> Result<T>) -> Result<(T, Option<String>)> {
    let mut pw = password.map(str::to_string);
    for attempt in 0..4 {
        match job(pw.as_deref()) {
            Err(e) if e.is_password() && attempt < 3 => {
                let wrong = matches!(e, Error::WrongPassword) || pw.is_some();
                match ask_password(ctl, archive, first_item, wrong) {
                    Some(p) => pw = Some(p),
                    None => return Err(e),
                }
            }
            Err(e) => return Err(e),
            Ok(v) => return Ok((v, pw)),
        }
    }
    Err(Error::WrongPassword)
}

pub fn extract(archive: &Path, opts: &ExtractOptions, password: Option<&str>, ctl: &Ctl) -> Result<ExtractReport> {
    let label = display_name(archive);
    ctl.archive(&archive.display().to_string());
    ctl.stage("Reading the item list");
    let fmt = reader::detect(archive)?;
    // Know the sizes up front (cheap for zip/7z/rar), and the first locked item for the password question.
    let mut first_locked = String::new();
    if !fmt.is_tar_family() {
        let (listing, pw) = with_password(archive, password, ctl, "", |p| reader::list(archive, p))?;
        let wanted: Vec<&Entry> = listing.entries.iter().filter(|e| opts.wants(&e.path)).collect();
        ctl.set_totals(wanted.len() as u64, wanted.iter().map(|e| e.size).sum());
        if let Some(e) = wanted.iter().find(|e| e.encrypted) {
            first_locked = e.path.clone();
        }
        if wanted.is_empty() {
            ctl.info(&label, "there was nothing to unpack");
            return Ok(ExtractReport::default());
        }
        let password = pw.or(password.map(str::to_string));
        return extract_inner(archive, opts, password.as_deref(), ctl, &first_locked, &label);
    }
    ctl.set_totals(0, 0);
    extract_inner(archive, opts, password, ctl, &first_locked, &label)
}

fn extract_inner(archive: &Path, opts: &ExtractOptions, password: Option<&str>, ctl: &Ctl, first_locked: &str, label: &str) -> Result<ExtractReport> {
    fs::create_dir_all(&opts.dest)?;
    let dest = fs::canonicalize(&opts.dest).unwrap_or_else(|_| opts.dest.clone());
    let mut overwrite = opts.overwrite;
    let done: std::cell::RefCell<HashSet<String>> = Default::default();
    let mut report = ExtractReport::default();
    ctl.stage("Unpacking");

    let ((), used) = with_password(archive, password, ctl, first_locked, |pw| {
        let mut want = |e: &Entry| opts.wants(&e.path) && !done.borrow().contains(&e.path);
        let mut f = |e: &Entry, r: &mut dyn Read| -> Result<()> {
            ctl.check()?;
            ctl.item(&e.path, e.size);
            let res = extract_one(e, r, &dest, opts, &mut overwrite, ctl, label, &mut report);
            match res {
                Ok(()) => {
                    done.borrow_mut().insert(e.path.clone());
                    Ok(())
                }
                Err(err) if err.is_password() || matches!(err, Error::Cancelled) => Err(err),
                Err(err) => {
                    report.errors += 1;
                    ctl.error(label, format!("{}: {err}", e.path));
                    done.borrow_mut().insert(e.path.clone());
                    Ok(())
                }
            }
        };
        reader::visit(archive, pw, &mut want, &mut f)
    })?;
    report.password_used = used;
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
fn extract_one(e: &Entry, r: &mut dyn Read, dest: &Path, opts: &ExtractOptions, overwrite: &mut Overwrite, ctl: &Ctl, label: &str, report: &mut ExtractReport) -> Result<()> {
    if e.is_link {
        ctl.info(label, format!("link \"{}\" was not unpacked (links are never followed or created)", e.path));
        report.skipped += 1;
        return Ok(());
    }
    if !e.is_dir && matches_any(&opts.blocked, &e.path) {
        ctl.warn(label, format!("\"{}\" was not unpacked: this file type is blocked in the security settings", e.path));
        report.skipped += 1;
        return Ok(());
    }
    // Path below the current folder of the archive, then made safe.
    let base = opts.base.trim_matches('/');
    let rel_name = if !base.is_empty() && e.path.starts_with(base) && e.path.as_bytes().get(base.len()) == Some(&b'/') { &e.path[base.len() + 1..] } else { e.path.as_str() };
    let rel_name = match opts.paths {
        ExtractPaths::Relative => rel_name.to_string(),
        ExtractPaths::NoPaths => {
            if e.is_dir {
                return Ok(());
            }
            e.name().to_string()
        }
    };
    let stored_ok = safe_relative(&e.raw).is_some();
    let Some(rel) = safe_relative(&rel_name).filter(|_| stored_ok) else {
        report.refused += 1;
        ctl.error(label, format!("\"{}\" was refused: its path would leave the destination folder", e.path));
        return Ok(());
    };
    if has_link_ancestor(dest, &rel) {
        report.refused += 1;
        ctl.error(label, format!("\"{}\" was refused: a folder on its way is a link that could lead outside the destination", e.path));
        return Ok(());
    }
    let mut target = dest.join(&rel);
    if e.is_dir {
        if fs::symlink_metadata(&target).is_ok_and(|m| m.file_type().is_symlink()) {
            report.refused += 1;
            ctl.error(label, format!("\"{}\" was refused: a link with that name is already there", e.path));
            return Ok(());
        }
        fs::create_dir_all(&target)?;
        report.folders += 1;
        return Ok(());
    }
    let existing = fs::symlink_metadata(&target).ok();
    if let Some(m) = &existing {
        if m.is_dir() {
            report.errors += 1;
            ctl.error(label, format!("\"{}\": a folder with that name is already there", e.path));
            return Ok(());
        }
        let old_time = m.modified().ok();
        let is_newer = newer(e.modified, old_time);
        if opts.update != ExtractUpdate::All && !is_newer {
            report.skipped += 1;
            return Ok(());
        }
        let action = match *overwrite {
            Overwrite::Always => Answer::Yes,
            Overwrite::Skip => Answer::No,
            Overwrite::Rename => Answer::Rename,
            Overwrite::Ask => ctl.ask(Question::Replace {
                path: target.display().to_string(),
                old_size: Some(m.len()),
                old_time,
                new_size: e.size,
                new_time: e.modified,
            }),
        };
        match action {
            Answer::Yes => {}
            Answer::YesToAll => *overwrite = Overwrite::Always,
            Answer::No | Answer::Password(_) => {
                report.skipped += 1;
                return Ok(());
            }
            Answer::NoToAll => {
                *overwrite = Overwrite::Skip;
                report.skipped += 1;
                return Ok(());
            }
            Answer::Rename => target = unique_path(&target),
            Answer::RenameAll => {
                *overwrite = Overwrite::Rename;
                target = unique_path(&target);
            }
            Answer::Cancel => return Err(Error::Cancelled),
        }
    } else if opts.update == ExtractUpdate::ExistingOnly {
        report.skipped += 1;
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    // Write next to the target, then move into place: a failed item never destroys the old file.
    let part = target.with_file_name(format!(".{}.acpart", target.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
    let res = (|| -> Result<()> {
        let mut f = io::BufWriter::new(File::create(&part)?);
        let mut pr = ProgressReader { inner: r, ctl };
        io::copy(&mut pr, &mut f)?;
        let f = f.into_inner().map_err(|e| e.into_error())?;
        if opts.set_mtime
            && let Some(t) = e.modified
        {
            let _ = f.set_modified(t);
        }
        apply_mode(&f, e.mode);
        Ok(())
    })();
    match res {
        Ok(()) => {
            if fs::symlink_metadata(&target).is_ok_and(|m| m.file_type().is_symlink()) {
                // Replace the link itself, never what it points to.
                fs::remove_file(&target)?;
            }
            fs::rename(&part, &target)?;
            report.files += 1;
            Ok(())
        }
        Err(err) => {
            if opts.keep_broken && !err.is_password() && !matches!(err, Error::Cancelled) {
                let _ = fs::rename(&part, &target);
                ctl.warn(label, format!("\"{}\" is damaged; the partial file was kept", e.path));
            } else {
                let _ = fs::remove_file(&part);
            }
            Err(err)
        }
    }
}

// ---------------------------------------------------------------- test

#[derive(Debug, Default, Clone)]
pub struct TestReport {
    pub files: usize,
    pub bytes: u64,
    pub errors: usize,
}

/// Read every item and check it against its stored checksum.
pub fn test(archive: &Path, password: Option<&str>, ctl: &Ctl) -> Result<TestReport> {
    let label = display_name(archive);
    ctl.archive(&archive.display().to_string());
    ctl.stage("Checking");
    let fmt = reader::detect(archive)?;
    let mut first_locked = String::new();
    let mut password = password.map(str::to_string);
    if !fmt.is_tar_family() {
        let (listing, pw) = with_password(archive, password.as_deref(), ctl, "", |p| reader::list(archive, p))?;
        ctl.set_totals(listing.entries.len() as u64, listing.total_size());
        if let Some(e) = listing.entries.iter().find(|e| e.encrypted) {
            first_locked = e.path.clone();
        }
        if pw.is_some() {
            password = pw;
        }
    } else {
        ctl.set_totals(0, volumes::total_size(archive));
    }
    let mut report = TestReport::default();
    let checked: std::cell::RefCell<HashSet<String>> = Default::default();
    let res = with_password(archive, password.as_deref(), ctl, &first_locked, |pw| {
        let mut want = |e: &Entry| !checked.borrow().contains(&e.path);
        let mut f = |e: &Entry, r: &mut dyn Read| -> Result<()> {
            ctl.check()?;
            ctl.item(&e.path, e.size);
            let mut pr = ProgressReader { inner: r, ctl };
            match io::copy(&mut pr, &mut io::sink()) {
                Ok(n) => {
                    if !e.is_dir && !e.is_link && n != e.size && e.size != 0 {
                        report.errors += 1;
                        ctl.error(&label, format!("{}: size is {n} bytes, the archive says {}", e.path, e.size));
                    } else {
                        report.files += usize::from(!e.is_dir);
                        report.bytes += n;
                    }
                }
                Err(err) => {
                    let err = Error::from(err);
                    if err.is_password() || matches!(err, Error::Cancelled) {
                        return Err(err);
                    }
                    report.errors += 1;
                    ctl.error(&label, format!("{}: {err}", e.path));
                }
            }
            checked.borrow_mut().insert(e.path.clone());
            Ok(())
        };
        reader::visit(archive, pw, &mut want, &mut f)
    });
    match res {
        Ok(_) => {}
        Err(Error::Cancelled) => return Err(Error::Cancelled),
        Err(e) if e.is_password() => return Err(e),
        Err(e) => {
            report.errors += 1;
            ctl.error(&label, format!("{e}"));
        }
    }
    if let Some(par) = archivecraft_par2::find_for(archive) {
        match archivecraft_par2::verify(&par) {
            Ok(v) if v.damaged_slices == 0 => ctl.info(&label, "recovery data: the archive matches it"),
            Ok(v) => ctl.warn(&label, format!("recovery data: {} damaged block(s); {}", v.damaged_slices, if v.repairable { "Repair can fix this" } else { "too many to repair" })),
            Err(e) => ctl.warn(&label, format!("recovery data could not be read: {e}")),
        }
    }
    Ok(report)
}

// ---------------------------------------------------------------- preview, delete, convert, comment

pub const PREVIEW_LIMIT: u64 = 64 * 1024 * 1024;

pub fn read_entry(archive: &Path, name: &str, password: Option<&str>) -> Result<Vec<u8>> {
    reader::read_one(archive, name, password, PREVIEW_LIMIT)
}

/// Options matching an existing archive, for rewriting it.
fn options_like(listing: &Listing, password: Option<&str>) -> CreateOptions {
    let locked = listing.any_encrypted();
    CreateOptions {
        format: listing.format,
        level: if listing.methods.iter().all(|m| m.eq_ignore_ascii_case("stored") || m.eq_ignore_ascii_case("copy")) { 0 } else { 5 },
        password: if locked { password.map(str::to_string) } else { None },
        encrypt_names: listing.names_encrypted,
        solid: listing.solid && listing.format == Format::SevenZ,
        store_patterns: Vec::new(),
        ..CreateOptions::default()
    }
}

/// Rewrite `archive` keeping only the items `keep` accepts.
fn rewrite(archive: &Path, password: Option<&str>, keep: &dyn Fn(&Entry) -> bool, comment: Option<Option<&str>>, ctl: &Ctl) -> Result<usize> {
    if volumes::is_first_volume(archive) {
        return Err(Error::Unsupported("a split archive cannot be changed".into()));
    }
    let listing = reader::list(archive, password)?;
    if !listing.format.can_write() {
        return Err(Error::Unsupported(format!("{} archives can only be read", listing.format.label())));
    }
    if listing.any_encrypted() && password.is_none_or(|p| p.is_empty()) {
        return Err(Error::PasswordRequired);
    }
    let opts = options_like(&listing, password);
    let kept: Vec<&Entry> = listing.entries.iter().filter(|e| keep(e)).collect();
    ctl.set_totals(kept.len() as u64, kept.iter().map(|e| e.size).sum());
    let work = temp_beside(archive);
    let count = Arc::new(AtomicU64::new(0));
    let res = (|| -> Result<usize> {
        let mut sink = open_sink(&work, &opts, count.clone())?;
        let n = copy_existing(archive, password, keep, sink.as_mut(), &[], ctl, &count)?;
        match comment {
            Some(Some(c)) => sink.set_comment(c)?,
            Some(None) => {}
            None => {
                if let Some(c) = &listing.comment {
                    sink.set_comment(c)?;
                }
            }
        }
        sink.finish()?;
        Ok(n)
    })();
    match res {
        Ok(n) => {
            fs::rename(&work, archive)?;
            Ok(listing.entries.len() - n)
        }
        Err(e) => {
            let _ = fs::remove_file(&work);
            Err(e)
        }
    }
}

/// Remove items (files or whole folders) from an archive. Returns how many items were removed.
pub fn delete(archive: &Path, items: &[String], password: Option<&str>, ctl: &Ctl) -> Result<usize> {
    ctl.archive(&archive.display().to_string());
    ctl.stage("Removing items");
    let items: Vec<String> = items.iter().map(|s| normalize_name(s)).collect();
    let keep = |e: &Entry| !selection_contains(&items, &e.path) || items.is_empty();
    let (n, _) = with_password(archive, password, ctl, "", |pw| rewrite(archive, pw, &keep, None, ctl))?;
    Ok(n)
}

/// Replace (or remove, with an empty text) the archive comment. ZIP only.
pub fn set_comment(archive: &Path, comment: &str, password: Option<&str>, ctl: &Ctl) -> Result<()> {
    let fmt = reader::detect(archive)?;
    if !fmt.supports_comment() {
        return Err(Error::Unsupported(format!("{} archives cannot hold a comment", fmt.label())));
    }
    ctl.stage("Saving the comment");
    let c = if comment.is_empty() { Some(None) } else { Some(Some(comment)) };
    with_password(archive, password, ctl, "", |pw| rewrite(archive, pw, &|_| true, c, ctl))?;
    Ok(())
}

/// Copy everything from one archive into a new one of another format.
pub fn convert(src: &Path, dst: &Path, opts: &CreateOptions, password: Option<&str>, ctl: &Ctl) -> Result<usize> {
    ctl.archive(&dst.display().to_string());
    ctl.stage("Converting");
    if dst.exists() {
        return Err(Error::other(format!("{} already exists", dst.display())));
    }
    let (listing, pw) = with_password(src, password, ctl, "", |p| reader::list(src, p))?;
    let pw = pw.or(password.map(str::to_string));
    ctl.set_totals(listing.entries.len() as u64, listing.total_size());
    let count = Arc::new(AtomicU64::new(0));
    let res = (|| -> Result<usize> {
        let mut sink = open_sink(dst, opts, count.clone())?;
        let n = copy_existing(src, pw.as_deref(), &|_| true, sink.as_mut(), &opts.store_patterns, ctl, &count)?;
        if let Some(c) = opts.comment.clone().or(listing.comment.clone())
            && opts.format.supports_comment()
        {
            sink.set_comment(&c)?;
        }
        sink.finish()?;
        Ok(n)
    })();
    match res {
        Ok(n) => {
            if let Some(v) = opts.volume_size {
                volumes::split_file(dst, v)?;
            }
            Ok(n)
        }
        Err(e) => {
            let _ = fs::remove_file(dst);
            Err(e)
        }
    }
}

/// Suggested folder for "unpack to <name>/".
pub fn folder_for(archive: &Path) -> PathBuf {
    let name = display_name(archive);
    let lower = name.to_lowercase();
    let stripped = volumes::strip_volume_suffix(&lower).to_string();
    let mut stem = name[..stripped.len()].to_string();
    for ext in [".tar.gz", ".tar.xz", ".tar.zst", ".tar.zstd", ".tgz", ".txz", ".tzst", ".tar", ".zip", ".7z", ".rar", ".jar"] {
        if stem.to_lowercase().ends_with(ext) {
            stem.truncate(stem.len() - ext.len());
            break;
        }
    }
    if stem.is_empty() {
        stem = "unpacked".into();
    }
    archive.parent().unwrap_or(Path::new(".")).join(stem)
}

/// Default archive path for "add to <name>.<ext>".
pub fn archive_path_for(sources: &[PathBuf], format: Format) -> PathBuf {
    let first = sources.first().cloned().unwrap_or_else(|| PathBuf::from("archive"));
    let parent = first.parent().map(Path::to_path_buf).unwrap_or_default();
    let base = if sources.len() == 1 {
        let n = display_name(&first);
        match first.is_dir() {
            true => n,
            false => Path::new(&n).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(n),
        }
    } else {
        parent.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "archive".into())
    };
    parent.join(format!("{base}.{}", format.extension()))
}

/// Folders implied by the items' paths, plus folders stored explicitly.
pub fn all_folders(entries: &[Entry]) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for e in entries {
        if e.is_dir {
            set.insert(e.path.clone());
        }
        let mut p = e.path.as_str();
        while let Some(i) = p.rfind('/') {
            p = &p[..i];
            set.insert(p.to_string());
        }
    }
    set
}

/// Write bytes to a fresh file, refusing to replace anything.
pub fn write_new(path: &Path, data: &[u8]) -> Result<()> {
    let mut f = fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(data)?;
    Ok(())
}
