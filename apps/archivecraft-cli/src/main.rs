//! archivecraft-cli: the same operations as the ArchiveCraft window, from a terminal.

use archivecraft_engine::model::util::{human_size, parse_patterns, parse_size};
use archivecraft_engine::model::{
    Answer, CreateOptions, Ctl, ExtractOptions, ExtractPaths, ExtractUpdate, Format, Overwrite, PathMode, Question, Severity, TimeFilter, UpdateMode,
};
use archivecraft_engine::{find, formats, ops, repair, Error};
use clap::{Parser, Subcommand, ValueEnum};
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "archivecraft-cli", version, about = "Create, list, test, unpack and repair archives (ZIP, 7z, tar, tar.gz, tar.xz, tar.zst; RAR read-only when built with it).")]
struct Cli {
    /// Never ask questions; use the defaults (skip existing files, fail on missing passwords).
    #[arg(long, global = true)]
    yes_no_questions: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, ValueEnum)]
enum Fmt {
    Zip,
    #[value(name = "7z")]
    SevenZ,
    Tar,
    #[value(name = "tar.gz")]
    TarGz,
    #[value(name = "tar.xz")]
    TarXz,
    #[value(name = "tar.zst")]
    TarZst,
}

impl From<Fmt> for Format {
    fn from(f: Fmt) -> Format {
        match f {
            Fmt::Zip => Format::Zip,
            Fmt::SevenZ => Format::SevenZ,
            Fmt::Tar => Format::Tar,
            Fmt::TarGz => Format::TarGz,
            Fmt::TarXz => Format::TarXz,
            Fmt::TarZst => Format::TarZst,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum Paths {
    Relative,
    None,
    Full,
}

#[derive(Clone, Copy, ValueEnum)]
enum AddMode {
    Add,
    Update,
    Freshen,
    Ask,
    Skip,
    Sync,
}

#[derive(Clone, Copy, ValueEnum)]
enum Over {
    Ask,
    Always,
    Skip,
    Rename,
}

#[derive(Clone, Copy, ValueEnum)]
enum XUpdate {
    All,
    Newer,
    Existing,
}

#[derive(clap::Args)]
struct PackArgs {
    /// Archive type (default: from the archive's file name, else ZIP).
    #[arg(short, long, value_enum)]
    format: Option<Fmt>,
    /// 0 = store only … 9 = smallest.
    #[arg(short, long, default_value_t = 5, value_parser = clap::value_parser!(u32).range(0..=9))]
    level: u32,
    /// Dictionary size for 7z and tar.xz, e.g. 32m.
    #[arg(long)]
    dictionary: Option<String>,
    /// Lock with this password (AES-256; ZIP and 7z only).
    #[arg(short, long)]
    password: Option<String>,
    /// Also hide the item names behind the password (7z).
    #[arg(long)]
    encrypt_names: bool,
    /// Weak old-style ZIP locking, for very old tools only.
    #[arg(long)]
    legacy_zip_lock: bool,
    /// One solid block (7z).
    #[arg(long)]
    solid: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// Make a new archive, or add files to an existing one.
    #[command(alias = "a", alias = "add")]
    Create {
        archive: PathBuf,
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        #[command(flatten)]
        pack: PackArgs,
        /// Split into parts of this size, e.g. 700m or 4g.
        #[arg(short = 'v', long)]
        volume_size: Option<String>,
        #[arg(long, value_enum, default_value = "relative")]
        paths: Paths,
        /// Leave out matching names (repeatable; wildcards * and ?).
        #[arg(short = 'x', long)]
        exclude: Vec<String>,
        /// Store matching names without compressing (repeatable).
        #[arg(long)]
        store: Vec<String>,
        /// What to do with items already in the archive.
        #[arg(short, long, value_enum, default_value = "add")]
        update: AddMode,
        /// Archive comment (ZIP).
        #[arg(long)]
        comment: Option<String>,
        /// Check the archive once it is written.
        #[arg(short, long)]
        test: bool,
        /// Write PAR2 recovery data of this many percent next to the archive.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        recovery: Option<u32>,
        /// Remove the original files once everything is safely inside.
        #[arg(long)]
        delete_sources: bool,
        /// Only files changed within: day, week, month; or "old" for older than a month.
        #[arg(long)]
        changed: Option<String>,
    },
    /// Show what is inside.
    #[command(alias = "l", alias = "ls")]
    List {
        archive: PathBuf,
        #[arg(short, long)]
        password: Option<String>,
        /// Exact byte counts.
        #[arg(long)]
        bytes: bool,
    },
    /// Read every item and check it against its stored checksum.
    #[command(alias = "t")]
    Test {
        archive: PathBuf,
        #[arg(short, long)]
        password: Option<String>,
    },
    /// Unpack all items, or only the ones named.
    #[command(alias = "x", alias = "e")]
    Extract {
        archive: PathBuf,
        /// Items or folders inside the archive.
        items: Vec<String>,
        /// Where to unpack (default: a folder named after the archive, next to it).
        #[arg(short, long)]
        dest: Option<PathBuf>,
        /// Unpack right next to the archive.
        #[arg(long, conflicts_with = "dest")]
        here: bool,
        #[arg(short, long, value_enum, default_value = "ask")]
        overwrite: Over,
        #[arg(short, long, value_enum, default_value = "all")]
        update: XUpdate,
        /// Drop folders; names only.
        #[arg(long)]
        no_paths: bool,
        /// Keep partly unpacked files when an item is damaged.
        #[arg(long)]
        keep_broken: bool,
        #[arg(short, long)]
        password: Option<String>,
        /// Do not unpack these names (repeatable).
        #[arg(long)]
        block: Vec<String>,
    },
    /// Remove items from an archive.
    #[command(alias = "d", alias = "rm")]
    Delete {
        archive: PathBuf,
        #[arg(required = true)]
        items: Vec<String>,
        #[arg(short, long)]
        password: Option<String>,
    },
    /// Totals, ratio and type.
    #[command(alias = "i")]
    Info {
        archive: PathBuf,
        #[arg(short, long)]
        password: Option<String>,
    },
    /// Write one item to standard output.
    #[command(alias = "p", alias = "print")]
    Cat {
        archive: PathBuf,
        item: String,
        #[arg(short, long)]
        password: Option<String>,
    },
    /// Show, set or clear the archive comment (ZIP).
    Comment {
        archive: PathBuf,
        #[arg(long, conflicts_with = "clear")]
        set: Option<String>,
        #[arg(long)]
        clear: bool,
        #[arg(short, long)]
        password: Option<String>,
    },
    /// Copy an archive into another format.
    Convert {
        source: PathBuf,
        target: PathBuf,
        #[command(flatten)]
        pack: PackArgs,
        /// Password of the source archive.
        #[arg(long)]
        source_password: Option<String>,
    },
    /// Write PAR2 recovery data next to an archive (or all its parts).
    Recovery {
        archive: PathBuf,
        #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u32).range(1..=100))]
        percent: u32,
    },
    /// Fix a damaged archive: from recovery data when present, else by saving readable items.
    Repair {
        archive: PathBuf,
        /// Folder for the repaired archive (default: next to it).
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Treat the damaged file as this type.
        #[arg(long, value_enum)]
        treat_as: Option<Fmt>,
        #[arg(short, long)]
        password: Option<String>,
    },
    /// Search a folder (and the archives in it) by name and contents.
    Find {
        folder: PathBuf,
        /// Name patterns, e.g. "*.txt report*".
        #[arg(short, long, default_value = "*")]
        name: String,
        /// Text that must appear inside.
        #[arg(short, long, default_value = "")]
        text: String,
        #[arg(long)]
        match_case: bool,
        /// The text is hexadecimal bytes.
        #[arg(long)]
        hex: bool,
        /// Do not look inside archives.
        #[arg(long)]
        no_archives: bool,
        /// Only this folder, not the ones inside it.
        #[arg(long)]
        no_subfolders: bool,
    },
    /// Add (or remove) the ArchiveCraft submenu in your file manager's right-click menu (Linux).
    Integrate {
        /// Take it out again.
        #[arg(long, conflicts_with = "status")]
        remove: bool,
        /// Only show what is installed.
        #[arg(long)]
        status: bool,
        /// The ArchiveCraft program the menu should run (default: `archivecraft` next to this tool).
        #[arg(long)]
        app: Option<PathBuf>,
    },
    /// Measure packing and unpacking speed on this computer.
    Bench {
        #[arg(short, long, value_enum, default_value = "7z")]
        format: Fmt,
        #[arg(short, long, default_value_t = 5)]
        level: u32,
        #[arg(long, default_value_t = 32)]
        size_mb: usize,
    },
}

fn prompt(line: &str) -> String {
    eprint!("{line}");
    let _ = io::stderr().flush();
    let mut s = String::new();
    let _ = io::stdin().lock().read_line(&mut s);
    s.trim_end_matches(['\r', '\n']).to_string()
}

fn ctl(interactive: bool) -> Arc<Ctl> {
    if !interactive || !io::stdin().is_terminal() {
        return Ctl::new();
    }
    Ctl::with_asker(Box::new(|q| match q {
        Question::Replace { path, old_size, new_size, .. } => {
            let old = old_size.map(|s| human_size(s, true)).unwrap_or_default();
            let a = prompt(&format!(
                "\"{path}\" is already there ({old} bytes; incoming {} bytes).\nReplace? [y]es, [n]o, [A]ll, [N]one, [r]ename, [R]ename all, [c]ancel: ",
                human_size(new_size, true)
            ));
            match a.as_str() {
                "y" | "yes" => Answer::Yes,
                "A" | "all" => Answer::YesToAll,
                "N" | "none" => Answer::NoToAll,
                "r" => Answer::Rename,
                "R" => Answer::RenameAll,
                "c" | "C" => Answer::Cancel,
                _ => Answer::No,
            }
        }
        Question::Password { archive, item, wrong } => {
            if wrong {
                eprintln!("That password did not work.");
            }
            let what = if item.is_empty() { format!("\"{archive}\"") } else { format!("\"{item}\" in \"{archive}\"") };
            let p = prompt(&format!("Password for {what} (shown as you type; empty to stop): "));
            if p.is_empty() { Answer::Cancel } else { Answer::Password(p) }
        }
    }))
}

fn report(c: &Ctl) -> usize {
    let d = c.diags.lock().unwrap();
    for x in d.iter() {
        let tag = match x.severity {
            Severity::Info => "note",
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        eprintln!("{tag}: {} ({})", x.message, x.archive);
    }
    d.iter().filter(|x| x.severity == Severity::Error).count()
}

fn pack_opts(p: &PackArgs, archive: &Path) -> Result<CreateOptions, Error> {
    let format = match p.format {
        Some(f) => f.into(),
        None => Format::from_name(&archive.to_string_lossy()).filter(|f| f.can_write()).unwrap_or(Format::Zip),
    };
    Ok(CreateOptions {
        format,
        level: p.level,
        dictionary: match &p.dictionary {
            Some(d) => Some(parse_size(d).ok_or_else(|| Error::other(format!("\"{d}\" is not a size")))?.min(u32::MAX as u64) as u32),
            None => None,
        },
        password: p.password.clone(),
        encrypt_names: p.encrypt_names,
        zip_legacy_crypto: p.legacy_zip_lock,
        solid: p.solid,
        ..CreateOptions::default()
    })
}

fn run(cli: Cli) -> Result<usize, Error> {
    let interactive = !cli.yes_no_questions;
    match cli.cmd {
        Cmd::Create { archive, inputs, pack, volume_size, paths, exclude, store, update, comment, test, recovery, delete_sources, changed } => {
            let mut o = pack_opts(&pack, &archive)?;
            o.volume_size = match volume_size {
                Some(v) => Some(parse_size(&v).ok_or_else(|| Error::other(format!("\"{v}\" is not a size")))?),
                None => None,
            };
            o.path_mode = match paths {
                Paths::Relative => PathMode::Relative,
                Paths::None => PathMode::NoPaths,
                Paths::Full => PathMode::Full,
            };
            o.exclude = exclude.iter().flat_map(|s| parse_patterns(s)).collect();
            if !store.is_empty() {
                o.store_patterns = store.iter().flat_map(|s| parse_patterns(s)).collect();
            }
            o.update = match update {
                AddMode::Add => UpdateMode::AddReplace,
                AddMode::Update => UpdateMode::Update,
                AddMode::Freshen => UpdateMode::Freshen,
                AddMode::Ask => UpdateMode::Ask,
                AddMode::Skip => UpdateMode::SkipExisting,
                AddMode::Sync => UpdateMode::Sync,
            };
            o.comment = comment;
            o.test_after = test;
            o.recovery_percent = recovery.unwrap_or(0);
            o.delete_sources = delete_sources;
            o.time_filter = match changed.as_deref() {
                None => TimeFilter::Any,
                Some("day") => TimeFilter::LastDay,
                Some("week") => TimeFilter::LastWeek,
                Some("month") => TimeFilter::LastMonth,
                Some("old") => TimeFilter::OlderThanMonth,
                Some(x) => return Err(Error::other(format!("--changed takes day, week, month or old, not \"{x}\""))),
            };
            let c = ctl(interactive);
            let r = ops::create(&inputs, &archive, &o, &c)?;
            println!("{} item(s) added, {} kept, {} removed → {}", r.added, r.kept, r.removed, r.parts.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "));
            for f in &r.recovery_files {
                println!("recovery data: {}", f.display());
            }
            if test {
                println!("check: {}", if r.test_errors == 0 { "no problems found".to_string() } else { format!("{} problem(s)", r.test_errors) });
            }
            Ok(report(&c))
        }
        Cmd::List { archive, password, bytes } => {
            let l = formats::list(&archive, password.as_deref())?;
            println!("{:>14} {:>14}  {:16}  {:8}  Name", "Size", "Packed", "Modified", "CRC32");
            for e in &l.entries {
                let size = if e.is_dir { "folder".to_string() } else { human_size(e.size, bytes) };
                let packed = e.packed.filter(|_| !e.is_dir).map(|p| human_size(p, bytes)).unwrap_or_default();
                let crc = e.crc.map(|c| format!("{c:08X}")).unwrap_or_default();
                let lock = if e.encrypted { "*" } else { "" };
                println!("{size:>14} {packed:>14}  {:16}  {crc:8}  {}{lock}", archivecraft_engine::model::util::format_time(e.modified, false), e.path);
            }
            println!("{} file(s), {} folder(s), {} unpacked, {} on disk ({}%)", l.file_count(), l.folder_count(), human_size(l.total_size(), bytes), human_size(l.archive_size, bytes), l.ratio_percent());
            Ok(0)
        }
        Cmd::Test { archive, password } => {
            let c = ctl(interactive);
            let r = ops::test(&archive, password.as_deref(), &c)?;
            let errs = report(&c);
            if errs == 0 {
                println!("No problems found: {} file(s), {} checked.", r.files, human_size(r.bytes, false));
            } else {
                println!("{errs} problem(s) found.");
            }
            Ok(errs)
        }
        Cmd::Extract { archive, items, dest, here, overwrite, update, no_paths, keep_broken, password, block } => {
            let dest = match (dest, here) {
                (Some(d), _) => d,
                (None, true) => archive.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(".")),
                (None, false) => ops::folder_for(&archive),
            };
            let mut o = ExtractOptions::to(&dest);
            o.overwrite = match overwrite {
                Over::Ask if interactive && io::stdin().is_terminal() => Overwrite::Ask,
                Over::Ask => Overwrite::Skip,
                Over::Always => Overwrite::Always,
                Over::Skip => Overwrite::Skip,
                Over::Rename => Overwrite::Rename,
            };
            o.update = match update {
                XUpdate::All => ExtractUpdate::All,
                XUpdate::Newer => ExtractUpdate::NewerOnly,
                XUpdate::Existing => ExtractUpdate::ExistingOnly,
            };
            o.paths = if no_paths { ExtractPaths::NoPaths } else { ExtractPaths::Relative };
            o.keep_broken = keep_broken;
            o.blocked = block.iter().flat_map(|s| parse_patterns(s)).collect();
            o.only = items;
            let c = ctl(interactive);
            let r = ops::extract(&archive, &o, password.as_deref(), &c)?;
            println!("{} file(s) and {} folder(s) unpacked to {}; {} skipped, {} refused.", r.files, r.folders, dest.display(), r.skipped, r.refused);
            Ok(report(&c) + r.errors.min(1))
        }
        Cmd::Delete { archive, items, password } => {
            let c = ctl(interactive);
            let n = ops::delete(&archive, &items, password.as_deref(), &c)?;
            println!("{n} item(s) removed.");
            Ok(report(&c))
        }
        Cmd::Info { archive, password } => {
            let l = formats::list(&archive, password.as_deref())?;
            println!("{}", archive.display());
            println!("  type          {}", l.format.describe());
            println!("  methods       {}", l.methods.join(", "));
            println!("  files         {}", l.file_count());
            println!("  folders       {}", l.folder_count());
            println!("  unpacked      {} bytes", human_size(l.total_size(), true));
            println!("  packed        {} bytes", human_size(l.packed_size(), true));
            println!("  on disk       {} bytes in {} part(s)", human_size(l.archive_size, true), l.volumes);
            println!("  ratio         {}%", l.ratio_percent());
            println!("  locked        {}", if l.names_encrypted { "items and names" } else if l.any_encrypted() { "items" } else { "no" });
            println!("  solid         {}", if l.solid { "yes" } else { "no" });
            println!("  comment       {}", l.comment.as_deref().unwrap_or("none"));
            println!("  recovery data {}", if archivecraft_engine::par2::find_for(&archive).is_some() { "yes" } else { "no" });
            Ok(0)
        }
        Cmd::Cat { archive, item, password } => {
            let data = ops::read_entry(&archive, &item, password.as_deref())?;
            io::stdout().write_all(&data).map_err(Error::Io)?;
            Ok(0)
        }
        Cmd::Comment { archive, set, clear, password } => {
            let c = ctl(interactive);
            if clear || set.is_some() {
                ops::set_comment(&archive, set.as_deref().unwrap_or(""), password.as_deref(), &c)?;
                println!("Comment saved.");
            } else {
                println!("{}", formats::list(&archive, password.as_deref())?.comment.unwrap_or_default());
            }
            Ok(report(&c))
        }
        Cmd::Convert { source, target, pack, source_password } => {
            let o = pack_opts(&pack, &target)?;
            let c = ctl(interactive);
            let n = ops::convert(&source, &target, &o, source_password.as_deref(), &c)?;
            println!("{n} item(s) copied into {}.", target.display());
            Ok(report(&c))
        }
        Cmd::Recovery { archive, percent } => {
            let c = Ctl::new();
            let parts = formats::volumes::volume_parts(&archive);
            for f in archivecraft_engine::par2::create(&parts, percent, &c)? {
                println!("{}", f.display());
            }
            Ok(0)
        }
        Cmd::Repair { archive, out, treat_as, password } => {
            let out = out.unwrap_or_else(|| archive.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(".")));
            let c = Ctl::new();
            let r = repair::repair(&archive, &out, treat_as.map(Into::into), password.as_deref(), &c)?;
            match r.method {
                repair::RepairMethod::NothingToDo => println!("Nothing to repair."),
                repair::RepairMethod::RecoveryData => println!("Rebuilt from recovery data."),
                repair::RepairMethod::Salvage { saved, lost } => println!("Saved {saved} item(s); {lost} could not be read."),
            }
            for p in r.output {
                println!("{}", p.display());
            }
            Ok(report(&c))
        }
        Cmd::Find { folder, name, text, match_case, hex, no_archives, no_subfolders } => {
            let q = find::FindQuery {
                names: parse_patterns(&name),
                text,
                match_case,
                hex,
                subfolders: !no_subfolders,
                inside_archives: !no_archives,
                ..find::FindQuery::default()
            };
            let c = Ctl::new();
            let n = find::find(&folder, &q, &c, &mut |h| match h.inner {
                Some(i) => println!("{} ▸ {}", h.file.display(), i),
                None => println!("{}", h.file.display()),
            })?;
            eprintln!("{n} found.");
            Ok(0)
        }
        Cmd::Integrate { remove, status, app } => {
            use archivecraft_engine::integration::{self, Places};
            let places = Places::for_user().ok_or_else(|| Error::other("your user folders could not be found"))?;
            if !status {
                if remove {
                    for p in integration::uninstall(&places)? {
                        println!("removed {}", p.display());
                    }
                } else {
                    let exe = match app {
                        Some(a) => std::fs::canonicalize(&a).map_err(Error::Io)?,
                        None => {
                            let me = std::env::current_exe().map_err(Error::Io)?;
                            let name = if cfg!(windows) { "archivecraft.exe" } else { "archivecraft" };
                            me.with_file_name(name)
                        }
                    };
                    if !exe.is_file() {
                        return Err(Error::other(format!("{} was not found; build it or pass --app", exe.display())));
                    }
                    for p in integration::install(&exe, &places, integration::detect())? {
                        println!("wrote {}", p.display());
                    }
                }
            }
            let t = integration::installed(&places);
            println!("installed: menu entry {}, Nautilus {}, Dolphin {}, Nemo {}, Thunar {}", t.desktop_entry, t.nautilus, t.dolphin, t.nemo, t.thunar);
            Ok(0)
        }
        Cmd::Bench { format, level, size_mb } => {
            let c = Ctl::new();
            let r = archivecraft_engine::bench::run(format.into(), level, size_mb, archivecraft_engine::model::default_threads(), &c)?;
            println!("{} level {}: packs at {:.1} MB/s, unpacks at {:.1} MB/s, {} → {} ({:.0}%)", r.format.label(), r.level, r.pack_speed, r.unpack_speed, human_size(r.input, false), human_size(r.output, false), r.output as f64 * 100.0 / r.input as f64);
            Ok(0)
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::from(1),
        Err(e) => {
            eprintln!("archivecraft-cli: {e}");
            ExitCode::from(match e {
                Error::PasswordRequired | Error::WrongPassword => 3,
                Error::Cancelled => 4,
                _ => 2,
            })
        }
    }
}
