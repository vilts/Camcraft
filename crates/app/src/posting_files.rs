//! The shipped post scripts and machine library by name, read from disk
//! at run time through `camcraft_post::data_dir` (#2282): the desktop
//! executable holds no compiled-in post script or machine file. The
//! browser build (`wasm32`) has no disk and keeps a compiled-in copy.
//!
//! Provenance: our own registry; the file layout is the release
//! packages' `assets/posts` and `assets/machines` beside the executable.

use camcraft_post::data_dir::DataFiles;
use camcraft_post::data_scan::FolderScan;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// A shipped data file, by its file name inside its `assets/<kind>`
/// directory. A type of its own so no caller takes the name for the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssetFile(pub &'static str);

impl AsRef<str> for AssetFile {
    fn as_ref(&self) -> &str {
        self.0
    }
}

/// The default machine's file in `assets/machines`: a plain 3-axis VMC.
pub const DEFAULT_MCH_FILE: AssetFile = AssetFile("3axis-vmc.mch");

/// The post scripts that ship with the app (name, file in `assets/posts`),
/// read from disk at run time (#2282, [`builtin_post_text`]). `PostRef::Builtin` means
/// the first entry (the default document's post, the meaning of a
/// project with no post key); a named built-in addresses an entry by
/// name, which survives registry growth the way an index never could.
/// Entry 0 is Fanuc and stays Fanuc - the goldens and every pre-stage-B
/// file say so.
pub const BUILTINS: &[(&str, AssetFile)] = &[
    ("Fanuc", AssetFile("fanuc.cps")),
    ("Haas", AssetFile("haas.cps")),
    ("Siemens 840D", AssetFile("sinumerik.cps")),
    ("Heidenhain", AssetFile("heidenhain.cps")),
    ("Mazak", AssetFile("mazak.cps")),
    ("Tormach PathPilot", AssetFile("tormach.cps")),
    // #710: grbl-family controllers (hobby and small-shop routers and mills).
    ("grblHAL", AssetFile("grblhal.cps")),
    // Synthetos TinyG: the other grbl-family controller, no canned cycles, M0 after each tool change.
    ("TinyG", AssetFile("tinyg.cps")),
    ("Doosan DNM", AssetFile("doosan-dnm.cps")),
    ("Makera Carvera", AssetFile("carvera.cps")),
    ("Mach4", AssetFile("mach4.cps")),
    ("Fanuc Lathe", AssetFile("fanuc-lathe.cps")),
    ("Tormach PathPilot Lathe", AssetFile("tormach-lathe.cps")),
    // #671: the plasma dialect, from UM10720 §6-7 and the PathPilot
    // post guidelines; it serves plasma tables only (`PLASMA_POSTS`).
    ("Tormach PathPilot Plasma", AssetFile("tormach-plasma.cps")),
    // #533: the generic RS-274 wire EDM post; every EDM code is the
    // machine's `edm.codes` (#290 D5). It serves wire EDMs only.
    ("Generic ISO Wire EDM", AssetFile("wire-iso.cps")),
    // #617: the Sodick 2-axis wire post, its words from Sodick's 1998
    // operation manual (Mark 21 / EX21); not verified on a control.
    ("Sodick Wire EDM (Mark 21)", AssetFile("sodick-wire.cps")),
    // #542: the generic ISO-style sinker post; every EDM code is the
    // machine's `edm.codes` (#290 D5). It serves sinker EDMs only.
    ("Sinker EDM (ISO)", AssetFile("sinker-iso.cps")),
];

/// The built-in machine LIBRARY (M3.3 stage C), checker grammar - the
/// files the Setup pane's machine group lists by name, the way
/// `BUILTINS` lists posts. Same law as the post registry: addressed
/// by name (a Setup that names "Mill-turn" still names it after the
/// library grows), and entry 0 stays the plain VMC, because the
/// first-shipped default is what old sessions remember. Unlike posts,
/// a machine choice is not stored as a reference - the document
/// mounts the parsed machine itself - so the name here is a picker
/// label and an install-time default, never a file's identity.
pub const MACHINES: &[(&str, AssetFile)] = &[
    ("VMC 3-axis", AssetFile("vmc.mch")),
    ("Tilt-head 5-axis", AssetFile("tilthead.mch")),
    ("Mill-turn", AssetFile("millturn.mch")),
    ("Lathe", AssetFile("lathe.mch")),
    // #131: an inch machine; pairs with the "Tormach PathPilot" post.
    ("Tormach 770MX", AssetFile("tormach-770mx.mch")),
    // #170: the other Tormach mills, each from its own operator's manual.
    ("Tormach 770MX (ML)", AssetFile("tormach-770mx-ml.mch")),
    // #785: the 770MX with Tormach's microARC 4 (TD10749) as an A about X.
    ("Tormach 770MX + microARC 4", AssetFile("tormach-770mx-microarc4.mch")),
    ("Tormach 770M", AssetFile("tormach-770m.mch")),
    ("Tormach 1100MX", AssetFile("tormach-1100mx.mch")),
    ("Tormach 1500MX", AssetFile("tormach-1500mx.mch")),
    ("Tormach PCNC 440", AssetFile("tormach-pcnc440.mch")),
    ("Tormach 1100M", AssetFile("tormach-1100m.mch")),
    // #170: the Mach3-era R8 mills, each from its own operator's manual.
    ("Tormach PCNC 770", AssetFile("tormach-pcnc770.mch")),
    ("Tormach PCNC 770 (Series 3)", AssetFile("tormach-pcnc770-s3.mch")),
    ("Tormach PCNC 1100 (Series I)", AssetFile("tormach-pcnc1100-s1.mch")),
    ("Tormach PCNC 1100 (Series II)", AssetFile("tormach-pcnc1100-s2.mch")),
    ("Tormach PCNC 1100 (Series 3)", AssetFile("tormach-pcnc1100-s3.mch")),
    // #618: the M+ upgrade kits - the M's figures with the MX's axis speeds.
    ("Tormach 770M+", AssetFile("tormach-770m-plus.mch")),
    ("Tormach 1100M+", AssetFile("tormach-1100m-plus.mch")),
    // #710: a generic grbl-family router / mill; pairs with the "grblHAL" post.
    ("grblHAL 3-axis", AssetFile("grblhal-3axis.mch")),
    // A generic TinyG router / mill; pairs with the "TinyG" post.
    ("TinyG 3-axis", AssetFile("tinyg-3axis.mch")),
    // #620: the PathPilot routers, each from its own manual / spec sheet
    // (the plain xsTECH from its spec sheet and knowledge base: no manual).
    ("Tormach 24R", AssetFile("tormach-24r.mch")),
    ("Tormach xsTECH Pro", AssetFile("tormach-xstech-pro.mch")),
    ("Tormach xsTECH", AssetFile("tormach-xstech.mch")),
    // #669: the plasma table, from UM10720 §2 and the 03/24 spec sheet.
    ("Tormach 1300PL", AssetFile("tormach-1300pl.mch")),
    // #611: Tormach's lathes, from their operator manuals (UM10753, UM10225).
    ("Tormach 8L", AssetFile("tormach-8l.mch")),
    ("Tormach 15L Slant-PRO", AssetFile("tormach-15l-slant-pro.mch")),
    // #619: the 15LX (UM10860 §3.1).
    ("Tormach 15LX Slant-PRO", AssetFile("tormach-15lx-slant-pro.mch")),
    // #707: Makera's desktop mills, from Makera's product spec tables.
    ("Makera Carvera", AssetFile("makera-carvera.mch")),
    ("Makera Carvera Air", AssetFile("makera-carvera-air.mch")),
    // #868: the Z1 (manual quick-change, air blast); posted through the Carvera post.
    ("Makera Z1", AssetFile("makera-z1.mch")),
    // #728: Sienci's AltMill routers (SLB-EXT, grblHAL); pair with the "grblHAL" post.
    ("Sienci AltMill 2x4", AssetFile("sienci-altmill-2x4.mch")),
    ("Sienci AltMill 4x4", AssetFile("sienci-altmill-4x4.mch")),
    ("Sienci AltMill 4x8", AssetFile("sienci-altmill-4x8.mch")),
    // #614: Doosan DNM 5700 VMC (Fanuc 0i-MF). #1433: the plain 3-axis
    // machine, and the catalog's optional A table as its own entry (no
    // published nose_to_axis, so that one refuses until a setup states it).
    ("Doosan DNM 5700", AssetFile("doosan-dnm-3axis.mch")),
    ("Doosan DNM 5700 + rotary table", AssetFile("doosan-dnm.mch")),
    // #1976: public Haas specifications; configurations and sources in docs/decisions/1976-haas-machines.md.
    ("Haas VF-2", AssetFile("haas-vf2.mch")),
    ("Haas VF-2SS", AssetFile("haas-vf2ss.mch")),
    ("Haas Mini Mill", AssetFile("haas-mini-mill.mch")),
    ("Haas DT-1", AssetFile("haas-dt1.mch")),
    ("Haas DM-1", AssetFile("haas-dm1.mch")),
    // #1135: the empty sinker template (no `edm.codes`); pairs with the
    // "Sinker EDM (ISO)" post, which refuses by name until the user fills the codes.
    ("Sinker EDM 3-axis (template)", AssetFile("sinker-edm.mch")),
    // #1501: a Sodick wire EDM, X Y U V travels and taper from Sodick's published
    // ALC series specifications (docs/decisions/1501-sodick-alc400g.md).
    ("Sodick ALC400G", AssetFile("sodick-alc400g.mch")),
];

fn post_dir() -> &'static DataFiles {
    static F: OnceLock<DataFiles> = OnceLock::new();
    F.get_or_init(|| DataFiles::from_process("posts"))
}

#[cfg(not(target_arch = "wasm32"))]
fn machine_dir() -> &'static DataFiles {
    static F: OnceLock<DataFiles> = OnceLock::new();
    F.get_or_init(|| DataFiles::from_process("machines"))
}

/// The post and machine folders as last read (#2380): read at the first
/// call and again at [`rescan`] (the Refresh buttons), never per frame.
/// The scan itself is `camcraft_post::data_scan`; this cache binds it to
/// the tables above and to the folders of this process. A name that a
/// document needs and that has no file is still refused by name when the
/// text is read (`builtin_post_text`, `builtin_machine_text`, #2319).
type Scans = (FolderScan, FolderScan);
static SCANS: Mutex<Option<Scans>> = Mutex::new(None);
static SCAN_READS: AtomicU64 = AtomicU64::new(0);

/// How many times this process read the two folders (first use and each
/// [`rescan`]): the log and the tests see that a Refresh read them again.
#[must_use]
pub fn scan_reads() -> u64 {
    SCAN_READS.load(Ordering::Relaxed)
}

#[cfg(target_arch = "wasm32")]
fn read_scans() -> Scans {
    // The browser build has no folder: every table entry is compiled in.
    let all = |t: &[(&'static str, AssetFile)]| FolderScan { present: t.iter().map(|(n, _)| *n).collect(), ..FolderScan::default() };
    (all(BUILTINS), all(MACHINES))
}

#[cfg(not(target_arch = "wasm32"))]
fn read_scans() -> Scans {
    let one = |files: &DataFiles, ext: &str, table: &[(&'static str, AssetFile)], hidden: &[AssetFile]| match files.dir() {
        Ok(dir) => camcraft_post::data_scan::scan_folder(dir, ext, table, hidden),
        Err(why) => FolderScan { problem: Some(why), ..FolderScan::default() },
    };
    let posts = one(post_dir(), "cps", BUILTINS, &[]);
    let machines = one(machine_dir(), "mch", MACHINES, &[DEFAULT_MCH_FILE]);
    for (kind, s) in [("posts", &posts), ("machines", &machines)] {
        for note in &s.skipped {
            tracing::warn!("{kind} folder: {note}");
        }
        match &s.problem {
            Some(why) => tracing::error!("{kind} folder: {why}"),
            None => tracing::info!("{kind} folder: {} shipped, {} other file(s)", s.present.len(), s.extra.len()),
        }
    }
    (posts, machines)
}

fn counted_read() -> Scans {
    SCAN_READS.fetch_add(1, Ordering::Relaxed);
    read_scans()
}

fn with_scans<T>(f: impl FnOnce(&Scans) -> T) -> T {
    let mut g = SCANS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    f(g.get_or_insert_with(counted_read))
}

/// The post folder as last read (read at first use).
#[must_use]
pub fn post_scan() -> FolderScan {
    with_scans(|s| s.0.clone())
}

/// The machine folder as last read (read at first use).
#[must_use]
pub fn machine_scan() -> FolderScan {
    with_scans(|s| s.1.clone())
}

/// Read both folders again (the Refresh buttons call this).
pub fn rescan() {
    *SCANS.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(counted_read());
}

/// One read-once cell per table entry: the text lives in static storage
/// for the life of the process (no leak, no second copy), so a caller
/// gets a `&'static str` the way it did from the compiled-in copy.
type Cell = OnceLock<Result<String, String>>;
static POST_TEXT: [Cell; BUILTINS.len()] = [const { OnceLock::new() }; BUILTINS.len()];
static MACHINE_TEXT: [Cell; MACHINES.len()] = [const { OnceLock::new() }; MACHINES.len()];

fn read(kind: &'static str, file: AssetFile) -> Result<String, String> {
    #[cfg(target_arch = "wasm32")]
    {
        embedded(kind, file.0).map(str::to_owned).ok_or_else(|| format!("{} is not in this build", file.0))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let files = if kind == "posts" { post_dir() } else { machine_dir() };
        let got = files.dir().and_then(|dir| camcraft_post::data_dir::read_file(dir, file.0));
        // Once per file (the cells cache it): which file on disk a post or
        // machine came from, because a user's edit there changes the G-code.
        match (&got, files.path_of(file.0)) {
            (Ok(_), Ok(path)) => tracing::info!("read shipped {kind} file {}", path.display()),
            (Err(why), _) => tracing::error!("shipped {kind} file {}: {why}", file.0),
            (Ok(_), Err(_)) => {}
        }
        got
    }
}

fn cached(cell: &'static Cell, kind: &'static str, file: AssetFile) -> Result<&'static str, String> {
    cell.get_or_init(|| read(kind, file)).as_deref().map_err(Clone::clone)
}

/// The text of a shipped post script by its registry name, read once. A
/// name not in the registry, a missing directory or a missing file is a
/// refusal by name.
pub fn builtin_post_text(name: &str) -> Result<&'static str, String> {
    let i = BUILTINS
        .iter()
        .position(|(n, _)| *n == name)
        .ok_or_else(|| format!("built-in post {name} is not in the registry"))?;
    let (cell, (_, file)) = POST_TEXT.get(i).zip(BUILTINS.get(i)).ok_or_else(|| format!("built-in post {name}: no slot"))?;
    cached(cell, "posts", *file).map_err(|why| format!("built-in post {name}: {why}"))
}

/// Where a shipped post script is read from - the file a user sees as
/// the one that made the program (#2282). `None` in the browser build.
#[must_use]
pub fn builtin_post_path(name: &str) -> Option<PathBuf> {
    let (_, file) = BUILTINS.iter().find(|(n, _)| *n == name)?;
    if cfg!(target_arch = "wasm32") {
        return None;
    }
    post_dir().path_of(file.0).ok()
}

/// The text of a shipped machine file by its library name, read once.
pub fn builtin_machine_text(name: &str) -> Result<&'static str, String> {
    let i = MACHINES
        .iter()
        .position(|(n, _)| *n == name)
        .ok_or_else(|| format!("machine {name} is not in the built-in library"))?;
    let (cell, (_, file)) =
        MACHINE_TEXT.get(i).zip(MACHINES.get(i)).ok_or_else(|| format!("built-in machine {name}: no slot"))?;
    cached(cell, "machines", *file).map_err(|why| format!("built-in machine {name}: {why}"))
}

/// The default machine's text (`assets/machines/3axis-vmc.mch`), read
/// from disk at each call. Nothing is kept here: the post keeps only the
/// parsed machine that passed its check (`default_machine_checked`), so a
/// broken file that the user repairs posts without a restart (#2319).
pub fn default_mch_text() -> Result<String, String> {
    read("machines", DEFAULT_MCH_FILE).map_err(|why| format!("default machine: {why}"))
}

/// Every shipped post as (registry name, text), in registry order - or
/// the first that does not read. For tools and tests that walk the
/// registry; held in static storage, built once.
pub fn shipped_posts() -> Result<&'static [(&'static str, &'static str)], String> {
    static ALL: OnceLock<Result<Vec<(&'static str, &'static str)>, String>> = OnceLock::new();
    ALL.get_or_init(|| BUILTINS.iter().map(|(n, _)| builtin_post_text(n).map(|t| (*n, t))).collect())
        .as_deref()
        .map_err(Clone::clone)
}

/// Every shipped machine as (library name, text), in library order - or
/// the first that does not read. Built once, like [`shipped_posts`].
pub fn shipped_machines() -> Result<&'static [(&'static str, &'static str)], String> {
    static ALL: OnceLock<Result<Vec<(&'static str, &'static str)>, String>> = OnceLock::new();
    ALL.get_or_init(|| MACHINES.iter().map(|(n, _)| builtin_machine_text(n).map(|t| (*n, t))).collect())
        .as_deref()
        .map_err(Clone::clone)
}

#[cfg(any(target_arch = "wasm32", test))]
fn embedded(kind: &str, file: &str) -> Option<&'static str> {
    match (kind, file) {
        ("posts", "fanuc.cps") => Some(include_str!("../../../assets/posts/fanuc.cps")),
        ("posts", "haas.cps") => Some(include_str!("../../../assets/posts/haas.cps")),
        ("posts", "sinumerik.cps") => Some(include_str!("../../../assets/posts/sinumerik.cps")),
        ("posts", "heidenhain.cps") => Some(include_str!("../../../assets/posts/heidenhain.cps")),
        ("posts", "mazak.cps") => Some(include_str!("../../../assets/posts/mazak.cps")),
        ("posts", "tormach.cps") => Some(include_str!("../../../assets/posts/tormach.cps")),
        ("posts", "grblhal.cps") => Some(include_str!("../../../assets/posts/grblhal.cps")),
        ("posts", "tinyg.cps") => Some(include_str!("../../../assets/posts/tinyg.cps")),
        ("posts", "doosan-dnm.cps") => Some(include_str!("../../../assets/posts/doosan-dnm.cps")),
        ("posts", "carvera.cps") => Some(include_str!("../../../assets/posts/carvera.cps")),
        ("posts", "mach4.cps") => Some(include_str!("../../../assets/posts/mach4.cps")),
        ("posts", "fanuc-lathe.cps") => Some(include_str!("../../../assets/posts/fanuc-lathe.cps")),
        ("posts", "tormach-lathe.cps") => Some(include_str!("../../../assets/posts/tormach-lathe.cps")),
        ("posts", "tormach-plasma.cps") => Some(include_str!("../../../assets/posts/tormach-plasma.cps")),
        ("posts", "wire-iso.cps") => Some(include_str!("../../../assets/posts/wire-iso.cps")),
        ("posts", "sodick-wire.cps") => Some(include_str!("../../../assets/posts/sodick-wire.cps")),
        ("posts", "sinker-iso.cps") => Some(include_str!("../../../assets/posts/sinker-iso.cps")),
        ("machines", "vmc.mch") => Some(include_str!("../../../assets/machines/vmc.mch")),
        ("machines", "tilthead.mch") => Some(include_str!("../../../assets/machines/tilthead.mch")),
        ("machines", "millturn.mch") => Some(include_str!("../../../assets/machines/millturn.mch")),
        ("machines", "lathe.mch") => Some(include_str!("../../../assets/machines/lathe.mch")),
        ("machines", "tormach-770mx.mch") => Some(include_str!("../../../assets/machines/tormach-770mx.mch")),
        ("machines", "tormach-770mx-ml.mch") => Some(include_str!("../../../assets/machines/tormach-770mx-ml.mch")),
        ("machines", "tormach-770mx-microarc4.mch") => Some(include_str!("../../../assets/machines/tormach-770mx-microarc4.mch")),
        ("machines", "tormach-770m.mch") => Some(include_str!("../../../assets/machines/tormach-770m.mch")),
        ("machines", "tormach-1100mx.mch") => Some(include_str!("../../../assets/machines/tormach-1100mx.mch")),
        ("machines", "tormach-1500mx.mch") => Some(include_str!("../../../assets/machines/tormach-1500mx.mch")),
        ("machines", "tormach-pcnc440.mch") => Some(include_str!("../../../assets/machines/tormach-pcnc440.mch")),
        ("machines", "tormach-1100m.mch") => Some(include_str!("../../../assets/machines/tormach-1100m.mch")),
        ("machines", "tormach-pcnc770.mch") => Some(include_str!("../../../assets/machines/tormach-pcnc770.mch")),
        ("machines", "tormach-pcnc770-s3.mch") => Some(include_str!("../../../assets/machines/tormach-pcnc770-s3.mch")),
        ("machines", "tormach-pcnc1100-s1.mch") => Some(include_str!("../../../assets/machines/tormach-pcnc1100-s1.mch")),
        ("machines", "tormach-pcnc1100-s2.mch") => Some(include_str!("../../../assets/machines/tormach-pcnc1100-s2.mch")),
        ("machines", "tormach-pcnc1100-s3.mch") => Some(include_str!("../../../assets/machines/tormach-pcnc1100-s3.mch")),
        ("machines", "tormach-770m-plus.mch") => Some(include_str!("../../../assets/machines/tormach-770m-plus.mch")),
        ("machines", "tormach-1100m-plus.mch") => Some(include_str!("../../../assets/machines/tormach-1100m-plus.mch")),
        ("machines", "grblhal-3axis.mch") => Some(include_str!("../../../assets/machines/grblhal-3axis.mch")),
        ("machines", "tinyg-3axis.mch") => Some(include_str!("../../../assets/machines/tinyg-3axis.mch")),
        ("machines", "tormach-24r.mch") => Some(include_str!("../../../assets/machines/tormach-24r.mch")),
        ("machines", "tormach-xstech-pro.mch") => Some(include_str!("../../../assets/machines/tormach-xstech-pro.mch")),
        ("machines", "tormach-xstech.mch") => Some(include_str!("../../../assets/machines/tormach-xstech.mch")),
        ("machines", "tormach-1300pl.mch") => Some(include_str!("../../../assets/machines/tormach-1300pl.mch")),
        ("machines", "tormach-8l.mch") => Some(include_str!("../../../assets/machines/tormach-8l.mch")),
        ("machines", "tormach-15l-slant-pro.mch") => Some(include_str!("../../../assets/machines/tormach-15l-slant-pro.mch")),
        ("machines", "tormach-15lx-slant-pro.mch") => Some(include_str!("../../../assets/machines/tormach-15lx-slant-pro.mch")),
        ("machines", "makera-carvera.mch") => Some(include_str!("../../../assets/machines/makera-carvera.mch")),
        ("machines", "makera-carvera-air.mch") => Some(include_str!("../../../assets/machines/makera-carvera-air.mch")),
        ("machines", "makera-z1.mch") => Some(include_str!("../../../assets/machines/makera-z1.mch")),
        ("machines", "sienci-altmill-2x4.mch") => Some(include_str!("../../../assets/machines/sienci-altmill-2x4.mch")),
        ("machines", "sienci-altmill-4x4.mch") => Some(include_str!("../../../assets/machines/sienci-altmill-4x4.mch")),
        ("machines", "sienci-altmill-4x8.mch") => Some(include_str!("../../../assets/machines/sienci-altmill-4x8.mch")),
        ("machines", "doosan-dnm-3axis.mch") => Some(include_str!("../../../assets/machines/doosan-dnm-3axis.mch")),
        ("machines", "doosan-dnm.mch") => Some(include_str!("../../../assets/machines/doosan-dnm.mch")),
        ("machines", "haas-vf2.mch") => Some(include_str!("../../../assets/machines/haas-vf2.mch")),
        ("machines", "haas-vf2ss.mch") => Some(include_str!("../../../assets/machines/haas-vf2ss.mch")),
        ("machines", "haas-mini-mill.mch") => Some(include_str!("../../../assets/machines/haas-mini-mill.mch")),
        ("machines", "haas-dt1.mch") => Some(include_str!("../../../assets/machines/haas-dt1.mch")),
        ("machines", "haas-dm1.mch") => Some(include_str!("../../../assets/machines/haas-dm1.mch")),
        ("machines", "sinker-edm.mch") => Some(include_str!("../../../assets/machines/sinker-edm.mch")),
        ("machines", "sodick-alc400g.mch") => Some(include_str!("../../../assets/machines/sodick-alc400g.mch")),
        ("machines", "3axis-vmc.mch") => Some(include_str!("../../../assets/machines/3axis-vmc.mch")),
        _ => None,
    }
}

/// Review of #2293: the browser list holds every table file, with the
/// same text as on disk - a post added to a table but not to `embedded`
/// fails here, not only in the `wasm32` build.
#[cfg(test)]
#[test]
fn the_browser_list_matches_the_disk_tables() {
    let posts = BUILTINS.iter().map(|(_, f)| ("posts", f.0));
    let machines = MACHINES.iter().map(|(_, f)| ("machines", f.0)).chain([("machines", DEFAULT_MCH_FILE.0)]);
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    for (kind, file) in posts.chain(machines) {
        let disk = std::fs::read_to_string(assets.join(kind).join(file)).unwrap_or_else(|e| panic!("{kind}/{file}: {e}"));
        assert_eq!(embedded(kind, file), Some(disk.as_str()), "{kind}/{file} is missing from embedded() or differs");
    }
}
