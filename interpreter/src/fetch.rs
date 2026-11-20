//! Packages fetched with git, the lock and the cache (choices 1558, 1559, 1562-1564, 1571-1582).
//!
//! A package is the repository at `https://` and its address, its root at the root of the
//! repository (1575), at the tag of its version, `v1.4.0` (1557). It is fetched with the
//! system's `git` and nothing else (1558): into a bare repository, from which `sg` lists the
//! files, checks them and writes them itself (1573), so that no filter of the user's git runs and
//! no end of a line is turned. No code of a package runs while it is fetched (1559).
//!
//! The files of a package are hashed as a tree (1572) and kept in the cache, a folder for each
//! address, version and SHA-256 (1579, 1591). `sg.lock` beside `sg.project` holds the address, the version, the SHA-256 and
//! the `unsafe` of each package the program uses (1571); `sg lock`, `sg add`, `sg update` and
//! `sg remove` write it, choosing the versions by minimal version selection (1577, 1580), and
//! the other commands only read it. `SG_MIRROR` names a folder of bare repositories to take the
//! packages from in place of the network (1582).

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::diag::Span;
use crate::lexer::{self, Part, Tok};
use crate::project::{self, Project, Version};
use crate::sha256::{self, Sha256};

/// The name of the lock.
pub const LOCK: &str = "sg.lock";
/// The most a package may have in all, in one file, and the most files (choice 1574).
pub const MAX_BYTES: u64 = 64 << 20;
pub const MAX_FILE_BYTES: u64 = 16 << 20;
pub const MAX_FILES: usize = 10_000;
/// The most files and the most bytes git may list for a tag, the packages kept in its folders
/// too, before `sg` stops it (choice 1591): a tree of a few kilobytes can name billions of files.
/// The bound is twice the files a package may have.
const MAX_LISTED: usize = 20_000;
const MAX_LISTING_BYTES: usize = 8 << 20;

/// Set by `sg lsp`: a package the cache has not is not fetched (choice 1591). An editor that
/// opens a file is no reason to reach the network.
static CACHE_ONLY: AtomicBool = AtomicBool::new(false);

/// From now on, packages are taken from the cache alone.
pub fn fetch_nothing() {
    CACHE_ONLY.store(true, Ordering::Relaxed);
}

/// What the messages say to do when the lock does not fit `sg.project`.
pub const LOCK_AGAIN: &str = "`sg lock` writes it again from `sg.project`";

/// A line of `sg.lock` (choice 1571).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locked {
    pub address: String,
    pub version: Version,
    /// `sha256-` and 64 hexadecimal digits: the name of its folder in the cache too.
    pub sum: String,
    /// It has `unsafe` or `extern` in it (choice 1562).
    pub uses_unsafe: bool,
    /// The libraries of C its `extern` blocks link.
    pub libraries: Vec<String>,
}

impl Locked {
    fn line(&self) -> String {
        let mut line = format!("{} {} {}", self.address, self.version, self.sum);
        if self.uses_unsafe {
            line.push_str(" unsafe");
        }
        if !self.libraries.is_empty() {
            line.push_str(&format!(" c={}", self.libraries.join(",")));
        }
        line
    }

    fn same_package(&self, other: &Locked) -> bool {
        self.address == other.address && self.version.major == other.version.major
    }
}

/// The lock in `folder`; none when there is no `sg.lock`.
pub fn read_lock(folder: &Path) -> Result<Vec<Locked>, String> {
    let path = folder.join(LOCK);
    match std::fs::read_to_string(&path) {
        Ok(text) => parse_lock(&text, &path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!("cannot read {}: {error}", path.display())),
    }
}

pub fn parse_lock(text: &str, path: &Path) -> Result<Vec<Locked>, String> {
    let mut lock: Vec<Locked> = Vec::new();
    for (at, line) in text.lines().enumerate() {
        let fail = |why: &str| format!("{}:{}: {why}; {LOCK_AGAIN}", path.display(), at + 1);
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        let [address, version, sum, rest @ ..] = &words[..] else { return Err(fail("a line of the lock is an address, a version and a SHA-256")) };
        project::address_ok(address).map_err(|why| fail(&why))?;
        let version = project::version_of(version).map_err(|why| fail(&why))?;
        if !is_sum(sum) {
            return Err(fail(&format!("`{sum}` is not a SHA-256 as the lock writes it, `sha256-` and 64 hexadecimal digits")));
        }
        let mut entry = Locked { address: address.to_string(), version, sum: sum.to_string(), uses_unsafe: false, libraries: Vec::new() };
        for word in rest {
            match word.strip_prefix("c=") {
                Some(names) if entry.libraries.is_empty() && names.split(',').all(crate::check::is_library_name) => entry.libraries = names.split(',').map(str::to_string).collect(),
                None if *word == "unsafe" && !entry.uses_unsafe => entry.uses_unsafe = true,
                _ => return Err(fail(&format!("`{}` is not a word of the lock: after the SHA-256 come `unsafe` and `c=` and the libraries of C", project::printable(word)))),
            }
        }
        if lock.iter().any(|other| other.same_package(&entry)) {
            return Err(fail(&format!("{} {} has two lines", entry.address, entry.version.major)));
        }
        lock.push(entry);
    }
    Ok(lock)
}

fn is_sum(sum: &str) -> bool {
    sum.strip_prefix("sha256-").is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}

/// The text of a lock, its lines in the order of their addresses.
pub fn lock_text(lock: &[Locked]) -> String {
    let mut lines: Vec<&Locked> = lock.iter().collect();
    lines.sort_by(|a, b| (&a.address, a.version.major).cmp(&(&b.address, b.version.major)));
    let mut text = String::from("// Written by `sg`; `sg lock` writes it again from `sg.project`.\n");
    for entry in lines {
        text.push_str(&entry.line());
        text.push('\n');
    }
    text
}

/// The folder of the cache (choice 1563): `SG_CACHE`, or the system's folder for caches.
pub fn cache() -> Result<PathBuf, String> {
    let from = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from);
    let home = || from("HOME").ok_or_else(|| "there is no folder for the cache: HOME is not set, and SG_CACHE names none".to_string());
    let folder = match from("SG_CACHE") {
        Some(folder) => folder,
        None if cfg!(windows) => from("LOCALAPPDATA").ok_or("there is no folder for the cache: LOCALAPPDATA is not set, and SG_CACHE names none")?.join("sg"),
        None if cfg!(target_os = "macos") => home()?.join("Library").join("Caches").join("sg"),
        None => match from("XDG_CACHE_HOME").filter(|folder| folder.is_absolute()) {
            Some(folder) => folder.join("sg"),
            None => home()?.join(".cache").join("sg"),
        },
    };
    std::path::absolute(&folder).map_err(|error| format!("cannot find the folder of the cache {}: {error}", folder.display()))
}

/// Where the repository of a package is, and the ways git may reach it.
struct Origin {
    url: String,
    protocols: &'static str,
}

/// `https://` and the address (choice 1575), or the folder of the address in the mirror
/// `SG_MIRROR` names, which alone may be read as files (choice 1582).
fn origin(address: &str) -> Result<Origin, String> {
    match std::env::var_os("SG_MIRROR").filter(|value| !value.is_empty()) {
        Some(mirror) => {
            let mut path = std::path::absolute(PathBuf::from(&mirror)).map_err(|error| format!("cannot find the mirror SG_MIRROR names: {error}"))?;
            for part in address.split('/') {
                path.push(part);
            }
            if !path.is_dir() {
                return Err(format!("{address} is not in the mirror SG_MIRROR names: there is no folder {}", path.display()));
            }
            Ok(Origin { url: path.to_string_lossy().into_owned(), protocols: "file" })
        }
        None => Ok(Origin { url: format!("https://{address}"), protocols: "https:ssh" }),
    }
}

/// `git`, which may reach a repository only in the ways given, never asks for a password on
/// the terminal, and gives up on a transfer that stalls (choice 1573). The user's own settings
/// (credentials, `insteadOf`) are kept.
///
/// It runs in `place`, an empty folder `sg` made, and finds no repository there or above it: a
/// folder of the user's that holds the files of a repository (a `config` among them) is never
/// read, and a bare repository is one only when `--git-dir` names it. No `GIT_` variable of the
/// user's reaches it, since one may hold settings or name other places (choice 1591).
fn git(protocols: &str, place: &Path) -> Command {
    let mut command = Command::new("git");
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    command
        .current_dir(place)
        .env("GIT_CEILING_DIRECTORIES", place.parent().unwrap_or(place))
        .env("GIT_ALLOW_PROTOCOL", protocols)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .env("LANGUAGE", "C")
        .args(["-c", "safe.bareRepository=explicit", "-c", "http.lowSpeedLimit=1000", "-c", "http.lowSpeedTime=60"])
        .stdin(Stdio::null());
    command
}

/// `git` on the bare repository `repository`, named so, in the folder above it.
fn git_on(protocols: &str, repository: &Path) -> Command {
    let mut command = git(protocols, repository.parent().unwrap_or(repository));
    command.arg(format!("--git-dir={}", repository.display()));
    command
}

fn not_started(error: std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => "`git` is not found: `sg` fetches packages with git, which is installed where `sg` runs (git-scm.com)".to_string(),
        _ => format!("cannot run git: {error}"),
    }
}

/// What git writes, or why it could not: `what` and the last of git's own words.
fn run(command: &mut Command, what: &str) -> Result<Vec<u8>, String> {
    let out = command.output().map_err(not_started)?;
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr);
        let said = project::printable(said.trim().lines().rfind(|line| !line.trim().is_empty()).unwrap_or("").trim());
        return Err(match said.is_empty() {
            true => what.to_string(),
            false => format!("{what}: {said}"),
        });
    }
    Ok(out.stdout)
}

/// The versions the tags of a package give, `vX.Y.Z` only (choice 1577), from the least.
pub fn tags(address: &str) -> Result<Vec<Version>, String> {
    let origin = origin(address)?;
    let scratch = Scratch::new(&made_cache()?)?;
    let out = run(git(origin.protocols, &scratch.0).args(["ls-remote", "--tags", "--refs", "--", &origin.url]), &format!("cannot read the tags of {address}"))?;
    let text = String::from_utf8_lossy(&out);
    let mut found: Vec<Version> = text.lines().filter_map(|line| line.split('\t').nth(1)?.strip_prefix("refs/tags/v")).filter_map(|written| project::version_of(written).ok()).collect();
    found.sort();
    found.dedup();
    Ok(found)
}

/// A folder made in the cache for one fetch, taken away when it ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(cache: &Path) -> Result<Scratch, String> {
        use std::sync::atomic::AtomicUsize;
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |time| time.subsec_nanos());
        let folder = cache.join(format!("tmp-{}-{nanos}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
        // A folder made new, never one that was there.
        std::fs::create_dir(&folder).map_err(|error| format!("cannot make a folder in the cache {}: {error}", cache.display()))?;
        Ok(Scratch(folder))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A file of a package as git lists it.
struct Entry {
    path: String,
    object: String,
    size: u64,
}

/// The folder of the cache, made when it is not there.
fn made_cache() -> Result<PathBuf, String> {
    let cache = cache()?;
    std::fs::create_dir_all(&cache).map_err(|error| format!("cannot make the folder of the cache {}: {error}", cache.display()))?;
    Ok(cache)
}

/// The folder in the cache of a version of a package with these files: under its address and
/// version, so that files fetched from another address are never taken for it (choice 1591).
fn place_in_cache(cache: &Path, address: &str, version: Version, sum: &str) -> PathBuf {
    let mut folder = cache.to_path_buf();
    let parts: Vec<&str> = address.split('/').collect();
    for part in &parts[..parts.len() - 1] {
        folder.push(part);
    }
    folder.push(format!("{}@{version}", parts[parts.len() - 1]));
    folder.push(sum);
    folder
}

/// The folder of a package in the cache, with its SHA-256. With `sum` (what the lock holds) a
/// package the cache has is not fetched again, and one that is fetched must have that SHA-256:
/// a tag moved to other files is found here (choice 1558). A package goes into the cache only
/// once its files are checked, its SHA-256 is the lock's and its `package` line is its address
/// (choice 1591).
pub fn fetched(address: &str, version: Version, sum: Option<&str>) -> Result<(String, PathBuf), String> {
    let cache = cache()?;
    if let Some(sum) = sum {
        let folder = place_in_cache(&cache, address, version, sum);
        if folder.is_dir() {
            return Ok((sum.to_string(), folder));
        }
    }
    let what = format!("{address} {version}");
    if CACHE_ONLY.load(Ordering::Relaxed) {
        return Err(format!("{what} is not in the cache, and the language server fetches nothing: `sg` or `sg build` in the folder of the program fetches it"));
    }
    let tag = format!("v{version}");
    let cache = made_cache()?;
    let scratch = Scratch::new(&cache)?;
    let origin = origin(address)?;
    let repository = scratch.0.join("git");
    run(git(origin.protocols, &scratch.0).args(["init", "--bare", "-q", "--"]).arg(&repository), "cannot make a repository in the cache")?;
    let reference = format!("refs/tags/{tag}");
    // The objects are kept in their pack, never unpacked into a file each: `ls-tree -l` reads
    // the size of every file it lists, and opening a loose object for each is slow on Windows
    // (18,000 names of one loose file took 38 seconds there, of one packed 0.8).
    run(
        git_on(origin.protocols, &repository).args(["-c", "fetch.unpackLimit=1", "fetch", "-q", "--depth", "1", "--no-tags", "--", &origin.url, &format!("+{reference}:{reference}")]),
        &format!("cannot fetch the tag {tag} of {address}"),
    )?;
    let listing = listed(&repository, origin.protocols, &reference).map_err(|why| format!("cannot list the files of {what}: {why}"))?;
    let entries = checked(&listing).map_err(|why| format!("{what} is refused: {why}"))?;
    let blobs = read_blobs(&repository, origin.protocols, &entries).map_err(|why| format!("cannot read the files of {what}: {why}"))?;
    let mut files: Vec<(&str, &[u8])> = entries.iter().zip(&blobs).map(|(entry, bytes)| (entry.path.as_str(), bytes.as_slice())).collect();
    let found = tree_sum(&mut files);
    if let Some(sum) = sum {
        if sum != found {
            return Err(format!(
                "the tag {tag} of {address} has other files than {LOCK} holds ({found}, and the lock has {sum}): the tag was moved. If the new files are trusted, take the line of {address} out of {LOCK} and run `sg lock`"
            ));
        }
    }
    // The address the package says it has (choice 1565), before it is kept.
    let shown = format!("{what}: {}", project::FILE);
    let Some((_, own)) = files.iter().find(|(path, _)| *path == project::FILE) else {
        return Err(format!("{what} has no `{}`, which every package has", project::FILE));
    };
    let own = std::str::from_utf8(own).map_err(|_| format!("{shown} is not UTF-8"))?;
    match project::parse(own, Path::new(&shown))?.package {
        Some(said) if said == address => {}
        Some(said) => return Err(format!("{shown}: the package is {said}, and it was asked for as {address}")),
        None => return Err(format!("{shown}: the `{}` of a package says where it is fetched from: `package {address}`", project::FILE)),
    }
    let folder = place_in_cache(&cache, address, version, &found);
    if !folder.is_dir() {
        let written = scratch.0.join("files");
        written_strictly(&written, &files).map_err(|why| format!("{what} cannot be kept on this system: {why}"))?;
        let above = folder.parent().expect("a folder in the cache is in a folder");
        std::fs::create_dir_all(above).map_err(|error| format!("cannot make a folder in the cache {}: {error}", above.display()))?;
        match std::fs::rename(&written, &folder) {
            Ok(()) => sealed(&folder),
            // Another `sg` may have put the same files there first.
            Err(_) if folder.is_dir() => {}
            Err(error) => return Err(format!("cannot put {what} into the cache {}: {error}", folder.display())),
        }
    }
    Ok((found, folder))
}

/// What `git ls-tree` lists for the tree of `reference`, read as git writes it and stopped once
/// it lists more than a package could hold (choice 1591).
fn listed(repository: &Path, protocols: &str, reference: &str) -> Result<Vec<u8>, String> {
    let mut child = git_on(protocols, repository)
        .args(["ls-tree", "-r", "-l", "-z", "--full-tree", &format!("{reference}^{{tree}}")])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(not_started)?;
    let mut errors = child.stderr.take().ok_or("git gave no output")?;
    let said = std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = errors.by_ref().take(64 << 10).read_to_end(&mut text);
        String::from_utf8_lossy(&text).into_owned()
    });
    let mut out = child.stdout.take().ok_or("git gave no output")?;
    let mut listing = Vec::new();
    let mut records = 0;
    let mut block = [0u8; 64 << 10];
    let too_many = loop {
        let read = match out.read(&mut block) {
            Ok(0) => break false,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.to_string()),
        };
        records += block[..read].iter().filter(|byte| **byte == 0).count();
        listing.extend_from_slice(&block[..read]);
        if records > MAX_LISTED || listing.len() > MAX_LISTING_BYTES {
            break true;
        }
    };
    if too_many {
        let _ = child.kill();
        let _ = child.wait();
        return Err("git lists more than the 20,000 files `sg` reads of a tag, the packages kept in its folders too".into());
    }
    let status = child.wait().map_err(|error| error.to_string())?;
    let said = said.join().unwrap_or_default();
    if !status.success() {
        let said = project::printable(said.trim().lines().rfind(|line| !line.trim().is_empty()).unwrap_or("").trim());
        return Err(match said.is_empty() {
            true => "git stopped".to_string(),
            false => said,
        });
    }
    Ok(listing)
}

/// The files git lists, checked before any is read (choice 1574): no link or submodule, names
/// every system can write, no two that differ only in case, and within the limits. The folders
/// with an `sg.project` of their own are other packages and left out (choice 1581).
fn checked(listing: &[u8]) -> Result<Vec<Entry>, String> {
    let mut all = Vec::new();
    for record in listing.split(|byte| *byte == 0).filter(|record| !record.is_empty()) {
        let text = std::str::from_utf8(record).map_err(|_| "the name of a file in it is not UTF-8".to_string())?;
        let (head, path) = text.split_once('\t').ok_or("git listed its files in a way `sg` does not read")?;
        let words: Vec<&str> = head.split_whitespace().collect();
        let [mode, _, object, size] = words[..] else { return Err("git listed its files in a way `sg` does not read".into()) };
        all.push((mode, object, size, path));
    }
    // The folders of other packages, each with its `/`: a path is in one when a folder above it is.
    let nested: HashSet<&str> = all.iter().filter_map(|(.., path)| path.strip_suffix(project::FILE).filter(|folder| folder.ends_with('/'))).collect();
    let mut entries = Vec::new();
    let mut total: u64 = 0;
    for (mode, object, size, path) in all {
        if path.match_indices('/').any(|(at, _)| nested.contains(&path[..=at])) {
            continue;
        }
        // The name first: what is printed of it below can then be printed.
        if let Some(why) = path_problem(path) {
            return Err(format!("the name {path:?} {why}"));
        }
        match mode {
            "100644" | "100755" => {}
            "120000" => return Err(format!("{path} is a link, and a package has none")),
            "160000" => return Err(format!("{path} is a submodule, and a package has none")),
            other => return Err(format!("{path} is of a kind of file git calls {}", project::printable(other))),
        }
        let size: u64 = size.parse().map_err(|_| "git listed its files in a way `sg` does not read".to_string())?;
        if size > MAX_FILE_BYTES {
            return Err(format!("{path} has {size} bytes, more than the 16 MB a file of a package may have"));
        }
        total += size;
        if total > MAX_BYTES {
            return Err("its files have more than the 64 MB a package may have".into());
        }
        entries.push(Entry { path: path.to_string(), object: object.to_string(), size });
        if entries.len() > MAX_FILES {
            return Err("it has more than the 10,000 files a package may have".into());
        }
    }
    // A path and every folder above it, as Windows and macOS see them: without case. Two
    // entries of one name are one file anywhere.
    let mut folded: HashMap<String, &str> = HashMap::new();
    let mut files: HashSet<&str> = HashSet::new();
    for entry in &entries {
        if !files.insert(&entry.path) {
            return Err(format!("{} is in it twice", entry.path));
        }
        let ends = entry.path.match_indices('/').map(|(at, _)| at).chain([entry.path.len()]);
        for end in ends {
            let spelled = &entry.path[..end];
            let first = folded.entry(spelled.to_lowercase()).or_insert(spelled);
            if *first != spelled {
                return Err(format!("{first} and {spelled} differ only in case, and are one file on Windows and macOS"));
            }
        }
    }
    Ok(entries)
}

/// Why a path git gives cannot be a file of a package on every system, as Go's rules for the
/// files of a module have it, or cannot be printed as it is (choice 1591).
fn path_problem(path: &str) -> Option<&'static str> {
    for part in path.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Some("has an empty part, `.` or `..`");
        }
        if part.chars().any(|c| project::is_unseen(c) || c == '\t') {
            return Some("has a character that is not seen as itself: a control character, a mark of direction, one with no width or a space other than the space");
        }
        if part.chars().any(|c| matches!(c, '\\' | ':' | '"' | '*' | '<' | '>' | '?' | '|')) {
            return Some("has a character a name on Windows cannot have");
        }
        if part.ends_with(['.', ' ']) {
            return Some("has a part that ends in a dot or a space, which Windows takes away");
        }
        let stem = part.split('.').next().unwrap_or(part).to_ascii_lowercase();
        let kept = matches!(stem.as_str(), "con" | "prn" | "aux" | "nul" | "conin$" | "conout$" | "clock$")
            || ((stem.starts_with("com") || stem.starts_with("lpt")) && stem.chars().count() == 4 && stem.chars().nth(3).is_some_and(|c| c.is_ascii_digit() || matches!(c, '¹' | '²' | '³')));
        if kept {
            return Some("has a name Windows keeps for a device");
        }
        // `GIT~1`: the short name Windows may give another file.
        if let Some((_, after)) = stem.rsplit_once('~') {
            if !after.is_empty() && after.bytes().all(|b| b.is_ascii_digit()) {
                return Some("has a `~` and digits, which Windows writes as the short name of another file");
            }
        }
        if part.eq_ignore_ascii_case(".git") {
            return Some("has a part `.git`");
        }
    }
    None
}

/// The files written into the new folder `root`, each folder made once and each file made new,
/// and read back: a system that takes two names for one (case, `ſ` and `s`, the two ways of
/// writing `é`) is found here, whatever it takes for one (choice 1591).
fn written_strictly(root: &Path, files: &[(&str, &[u8])]) -> Result<(), String> {
    let at = |path: &str| -> PathBuf { path.split('/').fold(root.to_path_buf(), |folder, part| folder.join(part)) };
    let fail = |path: &str, error: std::io::Error| match error.kind() {
        std::io::ErrorKind::AlreadyExists => format!("{path} is the name of another of its files or folders here"),
        _ => format!("cannot write {path}: {error}"),
    };
    std::fs::create_dir(root).map_err(|error| fail(".", error))?;
    let mut made: HashSet<&str> = HashSet::new();
    for (path, bytes) in files {
        for (end, _) in path.match_indices('/') {
            let folder = &path[..end];
            if made.insert(folder) {
                std::fs::create_dir(at(folder)).map_err(|error| fail(folder, error))?;
            }
        }
        let place = at(path);
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&place).map_err(|error| fail(path, error))?;
        file.write_all(bytes).map_err(|error| fail(path, error))?;
        drop(file);
        let mut permissions = std::fs::metadata(&place).map_err(|error| fail(path, error))?.permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&place, permissions).map_err(|error| fail(path, error))?;
    }
    let mut found: Vec<String> = Vec::new();
    let mut folders = vec![(root.to_path_buf(), String::new())];
    while let Some((folder, name)) = folders.pop() {
        for entry in std::fs::read_dir(&folder).map_err(|error| fail(&name, error))? {
            let entry = entry.map_err(|error| fail(&name, error))?;
            let Ok(part) = entry.file_name().into_string() else { return Err(format!("a name in {name} is not UTF-8 here")) };
            let path = if name.is_empty() { part } else { format!("{name}/{part}") };
            match entry.file_type().map_err(|error| fail(&path, error))?.is_dir() {
                true => folders.push((entry.path(), path)),
                false => found.push(path),
            }
        }
    }
    found.sort();
    let mut wanted: Vec<String> = files.iter().map(|(path, _)| path.to_string()).collect();
    wanted.sort();
    if wanted != found {
        let differs = wanted.iter().zip(&found).find(|(want, have)| want != have).map(|(want, _)| want).or_else(|| wanted.get(found.len())).or_else(|| found.get(wanted.len()));
        return Err(format!("this system keeps the names of its files otherwise than the package writes them ({:?})", differs.map_or("", String::as_str)));
    }
    Ok(())
}

/// The folders of a package kept in the cache made read only too, as Go's are, so that no
/// program changes what every program of the user reads (choice 1591). Its files are read only
/// already.
fn sealed(folder: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut folders = vec![folder.to_path_buf()];
        while let Some(folder) = folders.pop() {
            if let Ok(read) = std::fs::read_dir(&folder) {
                folders.extend(read.flatten().filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir())).map(|entry| entry.path()));
            }
            let _ = std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555));
        }
    }
    #[cfg(not(unix))]
    let _ = folder;
}

/// The bytes of each file, from `git cat-file --batch`.
fn read_blobs(repository: &Path, protocols: &str, entries: &[Entry]) -> Result<Vec<Vec<u8>>, String> {
    let mut child = git_on(protocols, repository).args(["cat-file", "--batch"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(not_started)?;
    let mut input = child.stdin.take().ok_or("git took no input")?;
    let asked: String = entries.iter().map(|entry| format!("{}\n", entry.object)).collect();
    let writer = std::thread::spawn(move || {
        let _ = input.write_all(asked.as_bytes());
    });
    let mut out = BufReader::new(child.stdout.take().ok_or("git gave no output")?);
    let mut blobs = Vec::with_capacity(entries.len());
    let wrong = || "git gave the files in a way `sg` does not read".to_string();
    for entry in entries {
        let mut header = String::new();
        out.by_ref().take(256).read_line(&mut header).map_err(|error| error.to_string())?;
        let words: Vec<&str> = header.split_whitespace().collect();
        if words.len() != 3 || words[1] != "blob" || words[2].parse::<u64>().ok() != Some(entry.size) {
            return Err(wrong());
        }
        let mut bytes = vec![0; entry.size as usize];
        out.read_exact(&mut bytes).map_err(|error| error.to_string())?;
        let mut end = [0u8; 1];
        out.read_exact(&mut end).map_err(|error| error.to_string())?;
        blobs.push(bytes);
    }
    let _ = writer.join();
    let _ = child.wait();
    Ok(blobs)
}

/// The SHA-256 of a tree of files (choice 1572): in the byte order of their paths, each path, a
/// byte 0, its length in eight bytes from the most significant and its bytes.
pub fn tree_sum(files: &mut [(&str, &[u8])]) -> String {
    files.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut sum = Sha256::default();
    for (path, bytes) in files.iter() {
        sum.update(path.as_bytes());
        sum.update(&[0]);
        sum.update(&(bytes.len() as u64).to_be_bytes());
        sum.update(bytes);
    }
    format!("sha256-{}", sha256::hex(&sum.finish()))
}

/// The places in `range` of `text` with code only `unsafe` allows (choice 1562): the words
/// `unsafe` and `extern`, an `extern` with the library it names when it names one.
pub fn unsafe_words(text: &str, range: Span) -> Vec<(Span, &'static str, Option<String>)> {
    let mut found = Vec::new();
    if let Ok(tokens) = lexer::lex(text, range, false) {
        unsafe_tokens(text, &tokens, &mut found);
    }
    found
}

fn unsafe_tokens(text: &str, tokens: &[lexer::Token], found: &mut Vec<(Span, &'static str, Option<String>)>) {
    for (at, token) in tokens.iter().enumerate() {
        match &token.tok {
            Tok::Ident(word) if word == "unsafe" => found.push((token.span, "unsafe", None)),
            Tok::Ident(word) if word == "extern" => {
                let library = match tokens.get(at + 1).map(|next| &next.tok) {
                    Some(Tok::Str(name)) => Some(name.clone()),
                    _ => None,
                };
                found.push((token.span, "extern", library));
            }
            Tok::Interp(parts) => {
                for part in parts {
                    if let Part::Code(span, _) = part {
                        if let Ok(inner) = lexer::lex(text, *span, true) {
                            unsafe_tokens(text, &inner, found);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Whether the source files of a package in `folder` have `unsafe`, and the libraries of C they
/// link, for the lock.
fn scan(folder: &Path) -> (bool, Vec<String>) {
    let mut uses_unsafe = false;
    let mut libraries: Vec<String> = Vec::new();
    let mut folders = vec![folder.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(read) = std::fs::read_dir(&folder) else { continue };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|extension| extension == "sg") {
                let Ok(text) = std::fs::read_to_string(&path) else { continue };
                for (_, _, library) in unsafe_words(&text, Span::new(0, text.len())) {
                    uses_unsafe = true;
                    // Only a name the checker takes is written in the lock (choice 1591).
                    if let Some(library) = library.filter(|library| crate::check::is_library_name(library)) {
                        libraries.push(library);
                    }
                }
            }
        }
    }
    libraries.sort();
    libraries.dedup();
    (uses_unsafe, libraries)
}

/// The packages the program of `project` uses, with the versions minimal version selection
/// chooses (choice 1577): every version a `require` reaches is read, the newest of each package
/// is taken, and the lock keeps those reached through the versions taken. The packages a `local`
/// line takes from a folder have no line (choice 1578); what the old lock holds is not fetched
/// again.
pub fn resolve(project: &Project, old: &[Locked]) -> Result<Vec<Locked>, String> {
    type Key = (String, u64);
    let folder = project.path.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut locals: HashMap<Key, PathBuf> = HashMap::new();
    for require in &project.requires {
        if let Some(local) = project.locals.iter().find(|local| local.name == require.name) {
            locals.insert((require.address.clone(), require.version.major), crate::source::cleaned(&folder.join(&local.folder)));
        }
    }
    let mut newest: HashMap<Key, Version> = HashMap::new();
    let mut needs: HashMap<(String, Version), Vec<(String, Version)>> = HashMap::new();
    let mut found: HashMap<(String, Version), (String, PathBuf)> = HashMap::new();
    let mut waiting: Vec<(String, Version)> = project.requires.iter().map(|require| (require.address.clone(), require.version)).collect();
    let mut seen: HashSet<(String, Version)> = HashSet::new();
    while let Some((address, version)) = waiting.pop() {
        if !seen.insert((address.clone(), version)) {
            continue;
        }
        let key = (address.clone(), version.major);
        let best = newest.entry(key.clone()).or_insert(version);
        *best = (*best).max(version);
        let own = match locals.get(&key) {
            Some(place) => project::read(place)?.ok_or_else(|| format!("{} has no `{}`, which every package has, with its `package` line", place.display(), project::FILE))?,
            None => {
                let known = old.iter().find(|entry| entry.address == address && entry.version == version).map(|entry| entry.sum.as_str());
                let (sum, place) = fetched(&address, version, known)?;
                found.insert((address.clone(), version), (sum, place.clone()));
                project::read(&place)?.ok_or_else(|| format!("{address} {version} has no `{}`, which every package has", project::FILE))?
            }
        };
        match &own.package {
            Some(said) if *said == address => {}
            Some(said) => return Err(format!("{}: the package is {said}, and it was asked for as {address}", own.path.display())),
            None => return Err(format!("{}: the `{}` of a package says where it is fetched from: `package {address}`", own.path.display(), project::FILE)),
        }
        let wanted: Vec<(String, Version)> = own.requires.iter().map(|require| (require.address.clone(), require.version)).collect();
        waiting.extend(wanted.iter().cloned());
        needs.insert((address, version), wanted);
    }
    let mut reached: Vec<Key> = Vec::new();
    let mut next: Vec<Key> = project.requires.iter().map(|require| (require.address.clone(), require.version.major)).collect();
    while let Some(key) = next.pop() {
        if reached.contains(&key) {
            continue;
        }
        let version = newest[&key];
        next.extend(needs[&(key.0.clone(), version)].iter().map(|(address, version)| (address.clone(), version.major)));
        reached.push(key);
    }
    let mut lock = Vec::new();
    for key in reached {
        if locals.contains_key(&key) {
            continue;
        }
        let version = newest[&key];
        let (sum, place) = &found[&(key.0.clone(), version)];
        let (uses_unsafe, libraries) = scan(place);
        lock.push(Locked { address: key.0, version, sum: sum.clone(), uses_unsafe, libraries });
    }
    lock.sort_by(|a, b| (&a.address, a.version.major).cmp(&(&b.address, b.version.major)));
    Ok(lock)
}

/// A command of packages that went wrong: a command line that is not a usage, or a message.
pub enum Failure {
    Usage,
    Error(String),
}

impl From<String> for Failure {
    fn from(message: String) -> Failure {
        Failure::Error(message)
    }
}

/// `sg add`, `sg update`, `sg remove` and `sg lock` (choices 1564 and 1580), on the
/// `sg.project` of the current folder.
pub fn command(words: &[String]) -> Result<(), Failure> {
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    match words[..] {
        ["lock"] => lock(),
        ["add", spec] => add(spec),
        ["update"] => update(None),
        ["update", name] => update(Some(name)),
        ["remove", name] => remove(name),
        _ => Err(Failure::Usage),
    }
}

/// The text of `sg.project` here; with `new`, an empty one when there is none.
fn project_text(new: bool) -> Result<String, String> {
    match std::fs::read_to_string(project::FILE) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && new => Ok(String::new()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(format!("there is no `{}` in this folder", project::FILE)),
        Err(error) => Err(format!("cannot read {}: {error}", project::FILE)),
    }
}

fn parsed(text: &str) -> Result<Project, String> {
    project::parse(text, Path::new(project::FILE))
}

/// Whether the program allows the package `unsafe`: its own `require` line ends in `unsafe`.
fn allowed(project: &Project, entry: &Locked) -> bool {
    project.requires.iter().any(|require| require.address == entry.address && require.version.major == entry.version.major && require.allows_unsafe)
}

/// What changed between the old lock and the new, and what of the new uses `unsafe` that the
/// program does not allow yet (choice 1564).
fn report(old: &[Locked], new: &[Locked], project: &Project) {
    for entry in new {
        let what = match old.iter().find(|before| before.same_package(entry)) {
            None => "added".to_string(),
            Some(before) if before.version != entry.version => format!("{} before", before.version),
            Some(before) if before.sum != entry.sum => "other files than before".to_string(),
            Some(_) => continue,
        };
        let mut uses = String::new();
        if entry.uses_unsafe {
            uses.push_str(", uses `unsafe`");
        }
        if !entry.libraries.is_empty() {
            uses.push_str(&format!(", links the C libraries {}", entry.libraries.join(", ")));
        }
        println!("{} {} ({what}{uses})", entry.address, entry.version);
        if entry.uses_unsafe && !allowed(project, entry) {
            println!("    the program builds with it once a `require` line of it ends in `unsafe`: `require {} {} {} unsafe`", line_name(&entry.address), entry.address, entry.version);
        }
    }
    for before in old {
        if !new.iter().any(|entry| entry.same_package(before)) {
            println!("{} {} (taken away)", before.address, before.version);
        }
    }
}

/// The name a line of the program gives a package unless it says another: the last part of
/// its address, made one that can be written.
fn line_name(address: &str) -> String {
    let last = address.rsplit('/').next().unwrap_or(address);
    match project::is_name(last) {
        true => last.to_string(),
        false => project::suggested(last),
    }
}

/// Writes `sg.project` when it changed and the lock, which is taken away when it has nothing.
fn save(before: &str, text: &str, lock: &[Locked]) -> Result<(), String> {
    if before != text {
        replaced(project::FILE, text)?;
    }
    if lock.is_empty() {
        return match std::fs::remove_file(LOCK) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(format!("cannot take away {LOCK}: {error}")),
            _ => Ok(()),
        };
    }
    replaced(LOCK, &lock_text(lock))
}

/// The file `name` of this folder with `text` in it: written into a file made new beside it and
/// put in its place, so that a link there is replaced and not followed (choice 1591), and a file
/// is never half written.
fn replaced(name: &str, text: &str) -> Result<(), String> {
    use std::sync::atomic::AtomicUsize;
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    let fail = |error: std::io::Error| format!("cannot write {name}: {error}");
    let new = format!(".{name}.{}-{}.new", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed));
    let written = std::fs::OpenOptions::new().write(true).create_new(true).open(&new).and_then(|mut file| file.write_all(text.as_bytes()));
    if let Err(error) = written.and_then(|()| std::fs::rename(&new, name)) {
        let _ = std::fs::remove_file(&new);
        return Err(fail(error));
    }
    Ok(())
}

fn lock() -> Result<(), Failure> {
    let text = project_text(false)?;
    let project = parsed(&text)?;
    let old = read_lock(Path::new(""))?;
    let new = resolve(&project, &old)?;
    report(&old, &new, &project);
    save(&text, &text, &new)?;
    Ok(())
}

/// `sg add example.com/ada/xml@1.4`: the newest tag that fits, on a line of its own.
fn add(spec: &str) -> Result<(), Failure> {
    let (address, wanted) = match spec.split_once('@') {
        Some((address, wanted)) => (address, Some(wanted)),
        None => (spec, None),
    };
    project::address_ok(address)?;
    let fits: Vec<u64> = match wanted {
        None => Vec::new(),
        Some(wanted) => {
            let numbers: Option<Vec<u64>> = wanted.split('.').map(|part| part.parse().ok().filter(|_| part.bytes().all(|b| b.is_ascii_digit()))).collect();
            match numbers {
                Some(numbers) if (1..=3).contains(&numbers.len()) => numbers,
                _ => return Err(Failure::Error(format!("`{wanted}` is not a version to look for: it is `1`, `1.4` or `1.4.2`"))),
            }
        }
    };
    let text = project_text(true)?;
    let project = parsed(&text)?;
    let tags = tags(address)?;
    let chosen = tags.iter().rev().find(|version| [version.major, version.minor, version.patch].iter().zip(&fits).all(|(have, want)| have == want)).copied();
    let Some(chosen) = chosen else {
        let had: Vec<String> = tags.iter().map(|version| version.to_string()).collect();
        let had = match had.is_empty() {
            true => "it has no tag `vX.Y.Z`".to_string(),
            false => format!("its versions are {}", had.join(", ")),
        };
        return Err(Failure::Error(format!("{address} has no version that fits `{spec}`: {had}")));
    };
    if let Some(other) = project.requires.iter().find(|require| require.address == address && require.version.major == chosen.major) {
        return Err(Failure::Error(format!("{}:{}: {address} {} is required there already; `sg update {}` takes a newer version", project::FILE, other.line, chosen.major, other.name)));
    }
    let name = line_name(address);
    if let Some(other) = project.requires.iter().find(|require| require.name == name) {
        return Err(Failure::Error(format!(
            "{}:{}: `{name}` is the name of another package; write the line with a name of your own, `require {name}2 {address} {chosen}`, and run `sg lock`",
            project::FILE,
            other.line
        )));
    }
    let mut new_text = text.clone();
    if !new_text.is_empty() && !new_text.ends_with('\n') {
        new_text.push('\n');
    }
    new_text.push_str(&format!("require {name} {address} {chosen}\n"));
    let new_project = parsed(&new_text)?;
    let old = read_lock(Path::new(""))?;
    let new = resolve(&new_project, &old)?;
    report(&old, &new, &new_project);
    save(&text, &new_text, &new)?;
    Ok(())
}

/// `sg update [name]`: the newest version of the same major version of each package the program
/// names, or of the one named. A version that starts to use `unsafe` the program does not
/// allow stops it, and nothing is written (choice 1562).
fn update(name: Option<&str>) -> Result<(), Failure> {
    let text = project_text(false)?;
    let project = parsed(&text)?;
    let local = |require: &project::Require| project.locals.iter().any(|local| local.name == require.name);
    let targets: Vec<&project::Require> = match name {
        Some(name) => {
            let Some(require) = project.requires.iter().find(|require| require.name == name) else {
                return Err(Failure::Error(format!("there is no package `{name}` in {}", project::FILE)));
            };
            if local(require) {
                return Err(Failure::Error(format!("`{name}` is taken from a folder by its `local` line, and has no version to update")));
            }
            vec![require]
        }
        None => project.requires.iter().filter(|require| !local(require)).collect(),
    };
    let mut new_text = text.clone();
    for require in targets {
        let newest = tags(&require.address)?.into_iter().filter(|version| version.major == require.version.major).max();
        if let Some(newest) = newest.filter(|newest| *newest > require.version) {
            new_text = with_line(&new_text, require.line, Some(&with_version(&line_of(&new_text, require.line), &require.address, newest)));
        }
    }
    let new_project = parsed(&new_text)?;
    let old = read_lock(Path::new(""))?;
    let new = resolve(&new_project, &old)?;
    for entry in &new {
        let was_safe = old.iter().any(|before| before.same_package(entry) && !before.uses_unsafe);
        if entry.uses_unsafe && was_safe && !allowed(&new_project, entry) {
            return Err(Failure::Error(format!(
                "{} {} uses `unsafe`, which the version before did not, and nothing is written; to take it, a `require` line of it ends in `unsafe`: `require {} {} {} unsafe`",
                entry.address,
                entry.version,
                line_name(&entry.address),
                entry.address,
                entry.version
            )));
        }
    }
    if new == old && new_text == text {
        println!("every package is at its newest version");
        return Ok(());
    }
    report(&old, &new, &new_project);
    save(&text, &new_text, &new)?;
    Ok(())
}

/// `sg remove name`: its `require` line and its `local` line.
fn remove(name: &str) -> Result<(), Failure> {
    let text = project_text(false)?;
    let project = parsed(&text)?;
    let Some(require) = project.requires.iter().find(|require| require.name == name) else {
        return Err(Failure::Error(format!("there is no package `{name}` in {}", project::FILE)));
    };
    let mut lines: Vec<usize> = project.locals.iter().filter(|local| local.name == name).map(|local| local.line).collect();
    lines.push(require.line);
    lines.sort_unstable_by(|a, b| b.cmp(a));
    let mut new_text = text.clone();
    for line in lines {
        new_text = with_line(&new_text, line, None);
    }
    let new_project = parsed(&new_text)?;
    let old = read_lock(Path::new(""))?;
    let new = resolve(&new_project, &old)?;
    report(&old, &new, &new_project);
    save(&text, &new_text, &new)?;
    Ok(())
}

/// The line numbered `number` (from 1), without its end.
fn line_of(text: &str, number: usize) -> String {
    text.lines().nth(number - 1).unwrap_or("").to_string()
}

/// The text with its line `number` put in place of by `new`, or taken away.
fn with_line(text: &str, number: usize, new: Option<&str>) -> String {
    let mut out = String::new();
    for (at, line) in text.split_inclusive('\n').enumerate() {
        if at + 1 != number {
            out.push_str(line);
            continue;
        }
        if let Some(new) = new {
            out.push_str(new);
            out.push_str(if line.ends_with("\r\n") { "\r\n" } else if line.ends_with('\n') { "\n" } else { "" });
        }
    }
    out
}

/// A `require` line with another version after its address, the rest as it was written.
fn with_version(line: &str, address: &str, version: Version) -> String {
    let Some(at) = line.find(address) else { return line.to_string() };
    let after = at + address.len();
    let rest = &line[after..];
    let start = after + (rest.len() - rest.trim_start().len());
    let end = start + line[start..].find(char::is_whitespace).unwrap_or(line.len() - start);
    format!("{}{version}{}", &line[..start], &line[end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_the_lock() {
        let text = "// Written by `sg`; `sg lock` writes it again from `sg.project`.\nexample.com/ada/xml 1.4.0 sha256-0000000000000000000000000000000000000000000000000000000000000000\nexample.com/bo/zlib 1.2.0 sha256-1111111111111111111111111111111111111111111111111111111111111111 unsafe c=m,z\n";
        let lock = parse_lock(text, Path::new(LOCK)).unwrap();
        assert!(lock[1].uses_unsafe && lock[1].libraries == ["m", "z"] && !lock[0].uses_unsafe);
        assert_eq!(lock_text(&lock), text);
        let error = |text: &str| parse_lock(text, Path::new(LOCK)).err().unwrap_or_default();
        assert!(error("example.com/ada/xml 1.4.0").contains("an address, a version and a SHA-256"));
        assert!(error("example.com/ada/xml 1.4.0 sha256-00").contains("64 hexadecimal digits"));
        assert!(error("example.com/ada/xml 1.4.0 sha256-0000000000000000000000000000000000000000000000000000000000000000 safe").contains("not a word of the lock"));
        assert!(error("example.com/ada/xml 1.4.0 sha256-0000000000000000000000000000000000000000000000000000000000000000\nexample.com/ada/xml 1.5.0 sha256-0000000000000000000000000000000000000000000000000000000000000000").contains("two lines"));
        // The libraries are names the checker takes, and nothing printed comes out as it is.
        for words in ["c=z,", "c=$(id)", "c=-lz", "c=z\u{1b}[2K"] {
            let said = error(&format!("example.com/ada/xml 1.4.0 sha256-{} {words}", "0".repeat(64)));
            assert!(said.contains("not a word of the lock") && !said.contains('\u{1b}'), "{said}");
        }
    }

    #[test]
    fn hashes_a_tree_in_the_order_of_its_paths() {
        let mut one: Vec<(&str, &[u8])> = vec![("b.sg", b"two"), ("a/x.sg", b"one")];
        let mut two: Vec<(&str, &[u8])> = vec![("a/x.sg", b"one"), ("b.sg", b"two")];
        assert_eq!(tree_sum(&mut one), tree_sum(&mut two));
        // The length keeps a path and its bytes apart: `ab` + `c` is not `a` + `bc`.
        assert_ne!(tree_sum(&mut [("ab", b"c")]), tree_sum(&mut [("a", b"bc")]));
        // The SHA-256 of the bytes: "x.sg", 0, length 1 in eight bytes, "1".
        let mut sum = Sha256::default();
        sum.update(b"x.sg\0\0\0\0\0\0\0\0\x011");
        assert_eq!(tree_sum(&mut [("x.sg", b"1")]), format!("sha256-{}", sha256::hex(&sum.finish())));
    }

    #[test]
    fn refuses_names_not_every_system_has() {
        for bad in ["a/../b", "a//b", "con.sg", "lib/LPT1", "x:y", "a\\b", "name.", "name ", "GIT~1/x", ".git/config", "a\u{1}", "CONIN$", "conout$.sg", "COM\u{b9}.sg", "lpt\u{b2}", "a\u{202e}gs.exe", "zero\u{200b}width.sg", "tab\tbed.sg", "nb\u{a0}sp.sg", "tag\u{e0041}.sg"] {
            assert!(path_problem(bad).is_some(), "{bad}");
        }
        for good in ["xml/reader.sg", "console.sg", "com10.sg", "a~b.sg", ".gitignore", "README.md", "été.sg"] {
            assert!(path_problem(good).is_none(), "{good}");
        }
    }

    #[test]
    fn edits_a_line() {
        assert_eq!(with_version("require xml example.com/ada/xml 1.4.0 unsafe // why", "example.com/ada/xml", Version { major: 1, minor: 5, patch: 0 }), "require xml example.com/ada/xml 1.5.0 unsafe // why");
        assert_eq!(with_line("a\r\nb\r\nc", 2, Some("x")), "a\r\nx\r\nc");
        assert_eq!(with_line("a\nb\nc\n", 2, None), "a\nc\n");
        assert_eq!(with_line("a\nb\nc", 3, Some("x")), "a\nb\nx");
    }

    #[test]
    fn finds_unsafe_in_code_only() {
        let text = "// unsafe in a comment\nvar s = \"extern\"\nextern \"z\" {\n    int crc(int a)\n}\nvoid f() {\n    print($\"{unsafe p[0]}\")\n}\n";
        let found: Vec<(&str, Option<String>)> = unsafe_words(text, Span::new(0, text.len())).into_iter().map(|(_, word, library)| (word, library)).collect();
        assert_eq!(found, [("extern", Some("z".to_string())), ("unsafe", None)]);
    }

    #[test]
    fn writes_each_file_once_and_reads_them_back() {
        let root = std::env::temp_dir().join(format!("sg-written-strictly-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut files: Vec<(&str, &[u8])> = vec![("a/b/c.sg", b"1"), ("a/d.sg", b"2"), ("e.sg", b"3")];
        assert_eq!(written_strictly(&root, &files), Ok(()));
        assert!(std::fs::metadata(root.join("a/b/c.sg")).unwrap().permissions().readonly());
        // A folder that is there is not taken: the files of another package are never mixed in.
        assert!(written_strictly(&root, &files).unwrap_err().contains(". is the name of another"));
        let _ = make_writable_and_remove(&root);
        files.push(("e.sg", b"4"));
        assert!(written_strictly(&root, &files).unwrap_err().contains("e.sg is the name of another of its files"));
        let _ = make_writable_and_remove(&root);
    }

    fn make_writable_and_remove(root: &Path) -> std::io::Result<()> {
        let mut folders = vec![root.to_path_buf()];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(&folder)?.flatten() {
                let mut permissions = entry.metadata()?.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                permissions.set_readonly(false);
                std::fs::set_permissions(entry.path(), permissions)?;
                if entry.file_type()?.is_dir() {
                    folders.push(entry.path());
                }
            }
        }
        std::fs::remove_dir_all(root)
    }

    #[test]
    fn keeps_a_package_under_its_address_and_version() {
        let version = Version { major: 1, minor: 4, patch: 0 };
        let place = place_in_cache(Path::new("cache"), "example.com/ada/xml", version, "sha256-00");
        assert_eq!(place, Path::new("cache").join("example.com").join("ada").join("xml@1.4.0").join("sha256-00"));
    }
}
