//! Repair a damaged archive: with recovery data (PAR2) when it exists, otherwise by saving
//! every item that can still be read into a fresh archive.

use archivecraft_formats::{self as formats, open_sink};
use archivecraft_model::{CreateOptions, Ctl, Entry, Error, Format, Result};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub enum RepairMethod {
    /// Rebuilt exactly from recovery data.
    RecoveryData,
    /// Readable items copied into a new archive.
    Salvage { saved: usize, lost: usize },
    /// Nothing was wrong.
    NothingToDo,
}

#[derive(Debug, Clone)]
pub struct RepairReport {
    pub method: RepairMethod,
    pub output: Vec<PathBuf>,
}

/// `name.zip` → `name.repaired.zip` in `out_dir`.
pub fn repaired_name(archive: &Path, out_dir: &Path) -> PathBuf {
    let name = archive.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "archive".into());
    let fmt = Format::from_name(&name);
    let (stem, ext) = match fmt {
        Some(f) if name.to_lowercase().ends_with(&format!(".{}", f.extension())) => {
            let cut = name.len() - f.extension().len() - 1;
            (name[..cut].to_string(), format!(".{}", &name[cut + 1..]))
        }
        _ => (name.clone(), String::new()),
    };
    out_dir.join(format!("{stem}.repaired{ext}"))
}

/// Repair `archive`. `treat_as` overrides format detection for archives too damaged to recognise.
pub fn repair(archive: &Path, out_dir: &Path, treat_as: Option<Format>, password: Option<&str>, ctl: &Ctl) -> Result<RepairReport> {
    let label = archive.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    ctl.archive(&archive.display().to_string());
    ctl.stage("Looking for recovery data");
    if let Some(index) = archivecraft_par2::find_for(archive) {
        let v = archivecraft_par2::verify(&index)?;
        if v.damaged_slices == 0 {
            ctl.info(&label, "the archive matches its recovery data; nothing to repair");
            return Ok(RepairReport { method: RepairMethod::NothingToDo, output: Vec::new() });
        }
        if v.repairable {
            let out = archivecraft_par2::repair(&index, out_dir, ctl)?;
            ctl.info(&label, format!("rebuilt {} damaged block(s) from recovery data", v.damaged_slices));
            return Ok(RepairReport { method: RepairMethod::RecoveryData, output: out });
        }
        ctl.warn(&label, format!("{} damaged blocks, {} recovery blocks: not enough; saving what can be read instead", v.damaged_slices, v.recovery_slices));
    } else {
        ctl.info(&label, "no recovery data found; saving what can be read");
    }
    let format = treat_as.or_else(|| formats::detect::detect(archive)).ok_or_else(|| Error::Unsupported("the archive type cannot be recognised; choose it by hand".into()))?;
    if !format.can_write() {
        return Err(Error::Unsupported(format!("{} archives can be repaired only with recovery data", format.label())));
    }
    fs::create_dir_all(out_dir)?;
    let out = repaired_name(archive, out_dir);
    let opts = CreateOptions { format, password: password.map(str::to_string).filter(|_| format.supports_password()), store_patterns: Vec::new(), ..CreateOptions::default() };
    let count = Arc::new(AtomicU64::new(0));
    let mut sink = open_sink(&out, &opts, count)?;
    ctl.stage("Saving readable items");
    let (saved, lost) = if format == Format::Zip { archivecraft_formats::zipfmt::salvage(archive, sink.as_mut(), ctl, &label)? } else { salvage_generic(archive, password, sink.as_mut(), ctl, &label)? };
    sink.finish()?;
    ctl.info(&label, format!("saved {saved} item(s), lost {lost}"));
    Ok(RepairReport { method: RepairMethod::Salvage { saved, lost }, output: vec![out] })
}

/// Copy items until the archive stops being readable.
fn salvage_generic(archive: &Path, password: Option<&str>, sink: &mut dyn formats::Sink, ctl: &Ctl, label: &str) -> Result<(usize, usize)> {
    let mut saved = 0;
    let mut lost = 0;
    let mut want = |_: &Entry| true;
    let mut f = |e: &Entry, r: &mut dyn Read| -> Result<()> {
        ctl.check()?;
        if e.is_dir {
            sink.add_dir(&e.path, e.modified)?;
            saved += 1;
            return Ok(());
        }
        let mut buf = Vec::new();
        match r.read_to_end(&mut buf) {
            Ok(_) => {
                sink.add_file(&e.path, e.modified, buf.len() as u64, e.mode, false, &mut buf.as_slice())?;
                saved += 1;
            }
            Err(err) => {
                lost += 1;
                ctl.error(label, format!("\"{}\" is damaged: {err}", e.path));
            }
        }
        Ok(())
    };
    if let Err(e) = formats::visit(archive, password, &mut want, &mut f) {
        if matches!(e, Error::Cancelled) {
            return Err(e);
        }
        ctl.error(label, format!("reading stopped: {e}"));
        lost += 1;
    }
    Ok((saved, lost))
}
