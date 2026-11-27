//! The files of a program. A folder is a module: the files in it are one module, named by the
//! path of the folder from the root of the program (`shop/stock` is `shop.stock`). The root
//! itself is the module without a name, where `main` is.
//!
//! All the files are kept as one text, one after the other, so a position in that text says
//! which file it is in.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::ast::ModuleMap;
use crate::fetch;
use crate::project::{self, Project};

use crate::diag::{snippet, Span};
use crate::lexer::{lex, Part, Tok};

/// The module of the types the language writes in SG and brings along (`Duration`, `Instant`,
/// `DateTime`): seen from every module with nothing written. No folder of a program has this name.
pub const STD_MODULE: &str = "std";

/// The files of the language's own module, each with the types it brings: a program that names
/// one of them gets the file.
const STD_FILES: &[(&str, &str, &[&str])] = &[
    ("time.sg", include_str!("std/time.sg"), &["Duration", "Instant", "DateTime", "DateTimeOffset", "DayOfWeek", "DateError"]),
    ("random.sg", include_str!("std/random.sg"), &["Random", "SecureRandom"]),
    ("files.sg", include_str!("std/files.sg"), &["Directory", "DirectoryEntry", "DirectoryRoot", "DirectoryWatcher", "FileChange", "FileInfo", "FileKind", "MappedFile", "SeekFrom"]),
    ("path.sg", include_str!("std/path.sg"), &["Path"]),
    ("environment.sg", include_str!("std/environment.sg"), &["Environment", "VariableError"]),
    ("process.sg", include_str!("std/process.sg"), &["Command", "Process", "Pipeline", "ExitStatus", "ProcessError", "ProcessChange"]),
    ("terminal.sg", include_str!("std/terminal.sg"), &["Terminal", "TerminalSize", "RawMode"]),
    ("net.sg", include_str!("std/net.sg"), &["SocketAddress", "UdpSocket", "Received"]),
    ("signals.sg", include_str!("std/signals.sg"), &["Signal", "Signals", "SignalListener"]),
    ("atomic.sg", include_str!("std/atomic.sg"), &["MemoryOrder"]),
    ("encoding.sg", include_str!("std/encoding.sg"), &["Hex", "Base64", "Percent", "DecodeError"]),
    ("checksum.sg", include_str!("std/checksum.sg"), &["Crc32", "Adler32"]),
    ("hash.sg", include_str!("std/hash.sg"), &["Sha256", "Hmac", "Bytes"]),
    ("command_line.sg", include_str!("std/command_line.sg"), &["CommandLine", "Arguments", "CommandLineError"]),
    ("json.sg", include_str!("std/json.sg"), &["Json", "JsonValue", "JsonObject", "JsonOptions", "JsonError", "JsonErrorKind", "JsonReader", "JsonToken"]),
    ("log.sg", include_str!("std/log.sg"), &["Log", "LogLevel"]),
    ("unicode.sg", include_str!("std/unicode.sg"), &["UnicodeCategory", "NormalizationForm"]),
    ("regex.sg", include_str!("std/regex.sg"), &["Regex", "RegexError", "RegexErrorKind", "Match", "Captures"]),
    ("compression.sg", include_str!("std/compression.sg"), &["Deflate", "Zlib", "Gzip", "Compressor", "Decompressor", "CompressError"]),
    ("zip.sg", include_str!("std/zip.sg"), &["ZipReader", "ZipWriter", "ZipEntry", "ZipMethod", "ZipError"]),
    ("http.sg", include_str!("std/http.sg"), &["HttpConnection", "HttpRequest", "HttpOptions"]),
];

/// The functions of the built-in `File` that the language's own module does the work of
/// (`File.info(path)` and `File.createTemp()` are `FileInfo`'s, and their async versions): a program
/// that names `File` and one of them gets the file of `FileInfo`.
pub const STD_FILE_CALLS: &[&str] = &["info", "exists", "createTemp", "infoAsync", "existsAsync", "createTempAsync", "linkInfo", "linkInfoAsync", "mapRead", "mapReadAsync", "openTemp", "openTempAsync"];

/// `reader.info()` gives a `FileInfo` (1873), and `stream.lockAsync()` is done by `FileLock` (1983): a
/// program that names one and a stream (`File`, which opens one, or `StreamReader` and `StreamWriter`,
/// the types of one it is given) gets the file of `FileInfo`.
pub const STD_READER_CALLS: &[&str] = &["info", "lockAsync", "lockSharedAsync"];

/// The methods of the built-in `TcpStream` that the language's own module does the work of
/// (`stream.remoteAddress()` gives a `SocketAddress`, `stream.setNoDelay(true)` is a call of the
/// system there): a program that names one gets the file of `SocketAddress`.
pub const STD_STREAM_CALLS: &[&str] = &["remoteAddress", "localAddress", "closeSending", "setNoDelay", "setKeepAlive"];

/// The methods of `char` and `string` that the language's own module does the work of
/// (`c.category()` gives a `UnicodeCategory`, `text.normalize(form)` takes a `NormalizationForm`):
/// a program that names one gets the file of both.
pub const STD_CHAR_CALLS: &[&str] = &["category", "normalize", "isNormalized"];

/// Every type of the language's own module.
pub fn std_types() -> impl Iterator<Item = &'static str> {
    STD_FILES.iter().flat_map(|(_, _, types)| types.iter().copied())
}

/// The names written in `text` from `start` on, those in the code of a `$"..."` too (choice 1400).
fn names_in(text: &str, start: usize, named: &mut HashSet<String>) {
    names_within(text, Span::new(start, text.len()), 0, named);
}

/// The names of the code at `span`, inside `depth` texts. Texts nested deeper than the parser
/// takes are not looked into (1596): the parser refuses them, and each level lexed again would
/// cost the time of all those inside it and a frame of the stack.
fn names_within(text: &str, span: Span, depth: usize, named: &mut HashSet<String>) {
    let Ok(toks) = lex(text, span, depth > 0) else { return };
    for tok in toks {
        match tok.tok {
            Tok::Ident(name) => {
                named.insert(name);
            }
            Tok::Interp(parts) if depth < crate::parser::MAX_NESTING => {
                for part in parts {
                    if let Part::Code(code, _) = part {
                        names_within(text, code, depth + 1, named);
                    }
                }
            }
            _ => {}
        }
    }
}

pub struct SourceFile {
    /// As it is written in messages.
    pub path: String,
    /// The module the file belongs to; empty for the root.
    pub module: String,
    /// Where the file lies in the text of the program.
    pub start: usize,
    pub end: usize,
    /// Whether the test command runs the `test` blocks of this file.
    pub tested: bool,
    /// How the file writes the names of modules: the package it is in.
    pub map: Arc<ModuleMap>,
}

/// A package the program uses (choice 1552): from a folder a `local` line gives, or from the
/// cache at the version the lock holds (choice 1571).
pub struct Package {
    /// The name the program's `sg.project` gives it, which is the module of its root files; for
    /// a package only another uses, a name no file can reach (choice 1576).
    pub name: String,
    pub address: String,
    /// The version the lock holds; for one from a folder, the version its line asks for.
    pub version: project::Version,
    pub folder: PathBuf,
    /// The program names it on a `require` line of its own.
    pub named: bool,
    /// The program allows it `unsafe` code (choice 1562).
    pub allows_unsafe: bool,
    pub map: Arc<ModuleMap>,
}

pub struct Sources {
    pub text: String,
    pub files: Vec<SourceFile>,
    /// The folder the modules are looked for in; `None` for a program that is one text.
    root: Option<PathBuf>,
    /// The folders `-L` names (see `Program::lib_dirs`).
    pub lib_dirs: Vec<PathBuf>,
    /// How the program's own files write the names of modules.
    map: Arc<ModuleMap>,
    pub packages: Vec<Package>,
    /// The name of the program: its file without `.sg`, or its folder. The C of a build is
    /// written to a file of this name, and a library takes it.
    pub name: String,
}

impl Sources {
    /// A program that is one text.
    pub fn single(src: &str, path: &str) -> Sources {
        let file = SourceFile { path: path.to_string(), module: String::new(), start: 0, end: src.len(), tested: true, map: Arc::default() };
        let name = Path::new(path).file_stem().map_or_else(|| "program".to_string(), |stem| stem.to_string_lossy().into_owned());
        let mut sources = Sources { text: src.to_string(), files: vec![file], root: None, lib_dirs: Vec::new(), map: Arc::default(), packages: Vec::new(), name };
        sources.add_std();
        sources
    }

    /// The files of the language's own module, after the files of the program, each when one of
    /// them names a type of it.
    fn add_std(&mut self) {
        let mut named = HashSet::new();
        for file in &self.files {
            names_in(&self.text[..file.end], file.start, &mut named);
        }
        if named.contains("File") && STD_FILE_CALLS.iter().any(|name| named.contains(*name)) {
            named.insert("FileInfo".to_string());
        }
        if STD_READER_CALLS.iter().any(|name| named.contains(*name)) && (named.contains("File") || named.contains("StreamReader") || named.contains("StreamWriter")) {
            named.insert("FileInfo".to_string());
        }
        if STD_STREAM_CALLS.iter().any(|name| named.contains(*name)) {
            named.insert("SocketAddress".to_string());
        }
        if STD_CHAR_CALLS.iter().any(|name| named.contains(*name)) {
            named.insert("UnicodeCategory".to_string());
        }
        // A file of the module may name a type of another (a `FileInfo` holds a `DateTime`), so
        // the names of each file brought are looked at too, until nothing more is named.
        let mut added = vec![false; STD_FILES.len()];
        loop {
            let mut more = false;
            for (at, (file, text, types)) in STD_FILES.iter().enumerate() {
                if !added[at] && types.iter().any(|name| named.contains(*name)) {
                    added[at] = true;
                    more = true;
                    let start = self.add(format!("<built-in>/std/{file}"), STD_MODULE, text, false, Arc::default());
                    let file = &self.files[start];
                    names_in(&self.text[..file.end], file.start, &mut named);
                }
            }
            if !more {
                break;
            }
        }
    }

    fn add(&mut self, path: String, module: &str, text: &str, tested: bool, map: Arc<ModuleMap>) -> usize {
        if !self.text.is_empty() {
            self.text.push('\n');
        }
        let start = self.text.len();
        self.text.push_str(text);
        self.files.push(SourceFile { path, module: module.to_string(), start, end: self.text.len(), tested, map });
        self.files.len() - 1
    }

    pub fn file_at(&self, offset: usize) -> &SourceFile {
        self.files.iter().find(|file| offset <= file.end).unwrap_or_else(|| self.files.last().expect("a program has a file"))
    }

    /// Location, source line and a marker under the span, in the file the span is in.
    pub fn snippet(&self, span: Span) -> String {
        let file = self.file_at(span.start);
        let start = span.start.clamp(file.start, file.end) - file.start;
        let end = span.end.clamp(file.start, file.end) - file.start;
        snippet(Span::new(start, end), &self.text[file.start..file.end], &file.path)
    }

    /// The same program with the modules no file names read in after it, so every position
    /// stays where it was. `None` if there is no such module. This is how the hint for a name
    /// that was not found can say which `use` line is missing.
    pub fn with_every_module(&self) -> Option<Sources> {
        let root = self.root.as_ref()?;
        let mut modules = Vec::new();
        modules_under(root, "", &mut modules);
        modules.retain(|module| !self.files.iter().any(|file| file.module == *module));
        if modules.is_empty() {
            return None;
        }
        let files = self.files.iter().map(|file| SourceFile { path: file.path.clone(), module: file.module.clone(), map: file.map.clone(), ..*file }).collect();
        let mut all = Sources { text: self.text.clone(), files, root: None, lib_dirs: self.lib_dirs.clone(), map: self.map.clone(), packages: Vec::new(), name: self.name.clone() };
        // Named as `load` names them: without `./` in front.
        let base = if root == Path::new(".") { PathBuf::new() } else { root.clone() };
        for module in modules {
            let folder = module.split('.').fold(base.clone(), |folder, part| folder.join(part));
            for file in sg_files(&folder) {
                let text = std::fs::read_to_string(&file).ok()?;
                all.add(file.to_string_lossy().into_owned(), &module, &text, false, self.map.clone());
            }
        }
        Some(all)
    }
}

/// Each folder of `module` is there under `base` with its name written as the module writes it:
/// on a system whose names ignore case (macOS, Windows), `Zlib.compress` of the language's own
/// `Zlib` is not the folder `zlib`.
fn spelled_as(base: &Path, module: &str) -> bool {
    let mut dir = base.to_path_buf();
    for part in module.split('.') {
        let listed = if dir.as_os_str().is_empty() { Path::new(".") } else { dir.as_path() };
        let found = match std::fs::read_dir(listed) {
            Ok(entries) => entries.flatten().any(|entry| entry.file_name() == part && entry.path().is_dir()),
            Err(_) => false,
        };
        if !found {
            return false;
        }
        dir = dir.join(part);
    }
    true
}

fn sg_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(entries) => entries.flatten().map(|entry| entry.path()).filter(|path| path.extension().is_some_and(|ext| ext == "sg") && path.is_file()).collect(),
        Err(_) => Vec::new(),
    };
    files.sort();
    files
}

/// The folders under `dir` that hold source files, as module names, with the folders inside
/// them. A folder reached again through a link (`a` -> `.`) is passed over: it is walked once,
/// by the first name it is found by (1596).
fn modules_under(dir: &Path, prefix: &str, found: &mut Vec<String>) {
    let mut walked = HashSet::new();
    walked.extend(std::fs::canonicalize(dir));
    modules_walk(dir, prefix, found, &mut walked);
}

fn modules_walk(dir: &Path, prefix: &str, found: &mut Vec<String>, walked: &mut HashSet<PathBuf>) {
    let mut folders: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(entries) => entries.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()).collect(),
        Err(_) => return,
    };
    folders.sort();
    for folder in folders {
        if !std::fs::canonicalize(&folder).is_ok_and(|real| walked.insert(real)) {
            continue;
        }
        let Some(name) = folder.file_name().and_then(|name| name.to_str()) else { continue };
        if !is_module_name(name) || is_project(&folder) {
            continue;
        }
        let module = if prefix.is_empty() { name.to_string() } else { format!("{prefix}.{name}") };
        if !sg_files(&folder).is_empty() {
            found.push(module.clone());
        }
        modules_walk(&folder, &module, found, walked);
    }
}

/// The modules `name` could be meant as in a file of `module`: under it, then under each module
/// it is inside, the closest first (`stock.units` for `units` in `stock`).
pub fn inner_modules(module: &str, name: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut prefix = module;
    while !prefix.is_empty() {
        found.push(format!("{prefix}.{name}"));
        prefix = prefix.rfind('.').map_or("", |at| &prefix[..at]);
    }
    found
}

/// A folder with an `sg.project` of its own is a project, not a module of the one around it, as
/// a folder with a `go.mod` is in Go: a package kept inside the program's folder is read as a
/// package, by its `local` line.
fn is_project(folder: &Path) -> bool {
    folder.join(project::FILE).is_file()
}

/// A folder is a module only if its name can be written in the language, and is not the name of
/// one of its types (choice 1591).
fn is_module_name(name: &str) -> bool {
    if name == STD_MODULE || crate::check::is_language_type(name) {
        return false;
    }
    let mut chars = name.chars();
    chars.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The modules a file names: after `use`, or in front of a dot (`billing.Line`). Every chain of
/// names is tried; whether a folder of that name exists is asked by the caller.
fn named_paths(text: &str, range: Span) -> Vec<String> {
    let Ok(toks) = lex(text, range, false) else { return Vec::new() };
    let mut paths = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let Tok::Ident(first) = &toks[i].tok else {
            i += 1;
            continue;
        };
        let after_use = i > 0 && toks[i - 1].tok == Tok::Use;
        let mut chain = vec![first.as_str()];
        i += 1;
        while i + 1 < toks.len() && toks[i].tok == Tok::Dot {
            let Tok::Ident(next) = &toks[i + 1].tok else { break };
            chain.push(next);
            i += 2;
        }
        // The last name of `billing.Line` is what is taken from the module; after `use` the
        // whole chain is the module.
        // A chain longer than a program may nest is an error of the parser; its paths are not
        // all tried, which would take memory as the square of its length.
        let longest = if after_use { chain.len() } else { chain.len() - 1 }.min(crate::parser::MAX_NESTING);
        let mut path = String::new();
        for name in &chain[..longest] {
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(name);
            paths.push(path.clone());
        }
    }
    paths
}

/// Reads the program at `path`. A file is a program of one file: it is the root module alone.
/// A folder is a program whose root module is every file in it. The folders next to the root
/// files are the modules; one is read when a file names it. With `every_module` all of them
/// are read and their tests run: that is what the test command does with a folder.
/// The name of the program a path holds: the file without `.sg`, or the folder, as the
/// folder is called even when the path is `.`.
pub fn program_name(path: &Path) -> String {
    let path = if path.is_dir() { std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()) } else { path.to_path_buf() };
    path.file_stem().map(crate::names::text_of_os).unwrap_or_else(|| "library".to_string())
}

pub fn load(path: &Path, every_module: bool) -> Result<Sources, String> {
    load_over(path, every_module, &HashMap::new())
}

/// The same, where a file `open` has is read from there and not from the disk: the text an
/// editor holds and has not saved (`sg lsp`, choice 1372).
pub fn load_over(path: &Path, every_module: bool, open: &HashMap<PathBuf, String>) -> Result<Sources, String> {
    let shown = |path: &Path| path.to_string_lossy().into_owned();
    let read = |path: &Path| match open.get(path) {
        Some(text) => Ok(text.clone()),
        None => std::fs::read_to_string(path).map_err(|error| format!("cannot read {}: {error}", shown(path))),
    };
    let mut sources = Sources { text: String::new(), files: Vec::new(), root: None, lib_dirs: Vec::new(), map: Arc::default(), packages: Vec::new(), name: program_name(path) };
    let (root, root_files) = if path.is_dir() {
        let files = sg_files(path);
        if files.is_empty() {
            return Err(format!("there is no `.sg` file in {}", shown(path)));
        }
        (path.to_path_buf(), files)
    } else {
        (path.parent().map(Path::to_path_buf).unwrap_or_default(), vec![path.to_path_buf()])
    };
    // `sg main.sg`: the modules are looked for in the current folder, and their files are named
    // as the main file is, `stock/product.sg` and not `./stock/product.sg` (choice 1401).
    let base = root.clone();
    let root = if root.as_os_str().is_empty() { PathBuf::from(".") } else { root };
    sources.root = Some(root.clone());
    if let Some(project) = project::read(&root)? {
        sources.packages = packages(&project, &base)?;
        let every: Arc<Vec<String>> = Arc::new(sources.packages.iter().map(|package| package.name.clone()).collect());
        sources.map = Arc::new(ModuleMap { prefix: String::new(), packages: sources.packages.iter().filter(|package| package.named).map(|package| (package.name.clone(), package.name.clone())).collect(), every_package: every.clone() });
        for package in &mut sources.packages {
            Arc::get_mut(&mut package.map).expect("not shared yet").every_package = every.clone();
        }
    }
    // A folder of modules named as a type of the language would take the calls of the type
    // (`Hex.encode`), in the program and inside the language's own module (choice 1591).
    for folder in std::iter::once(&base).chain(sources.packages.iter().map(|package| &package.folder)) {
        if let Some(found) = folder_named_as_a_type(folder) {
            return Err(format!("the folder {} has the name of a type of the language, and a folder of modules cannot have it: name it otherwise", shown(&found)));
        }
    }
    for file in root_files {
        sources.add(shown(&file), "", &read(&file)?, true, sources.map.clone());
    }

    // The folder a module is in, and how its files write the names of modules.
    let own_map = sources.map.clone();
    let places: Vec<(String, PathBuf, Arc<ModuleMap>)> = sources.packages.iter().map(|package| (package.name.clone(), package.folder.clone(), package.map.clone())).collect();
    let folder_of = |module: &str| -> Option<(PathBuf, Arc<ModuleMap>)> {
        let package = own_map.package_of(module);
        let (mut folder, map, rest) = match places.iter().find(|(name, _, _)| name == package && !package.is_empty()) {
            Some((_, folder, map)) => (folder.clone(), map.clone(), module[package.len()..].trim_start_matches('.')),
            None => (base.clone(), own_map.clone(), module),
        };
        if !rest.is_empty() {
            if !rest.split('.').all(is_module_name) || !spelled_as(&folder, rest) {
                return None;
            }
            for part in rest.split('.') {
                folder = folder.join(part);
                if is_project(&folder) {
                    return None;
                }
            }
        }
        Some((folder, map))
    };
    let mut wanted: Vec<String> = Vec::new();
    if every_module && path.is_dir() {
        modules_under(&root, "", &mut wanted);
    }
    let mut asked: HashMap<String, Option<(PathBuf, Arc<ModuleMap>)>> = HashMap::new();
    let mut loaded: HashSet<String> = HashSet::new();
    let mut scanned = 0;
    loop {
        // What the files read so far name.
        while scanned < sources.files.len() {
            let file = &sources.files[scanned];
            let mut exists = |module: &str| {
                asked.entry(module.to_string()).or_insert_with(|| folder_of(module).filter(|(folder, _)| !sg_files(folder).is_empty())).is_some()
            };
            for written in named_paths(&sources.text, Span::new(file.start, file.end)) {
                let module = file.map.full(&written);
                // A module of a package the file's own does not name is not read: the checker
                // says there is no such module.
                if !file.map.reaches(&module) {
                    continue;
                }
                if exists(&module) {
                    wanted.push(module);
                    continue;
                }
                // `use units` in `stock/` where `stock/units/` is: that module is read too, so
                // that the checker can say it is `use stock.units` (choice 1401).
                if let Some(inside) = inner_modules(&file.module, &written).into_iter().find(|inside| exists(inside)) {
                    wanted.push(inside);
                }
            }
            scanned += 1;
        }
        if wanted.is_empty() {
            break;
        }
        let module = wanted.remove(0);
        if !loaded.insert(module.clone()) {
            continue;
        }
        let Some((folder, map)) = asked.get(&module).cloned().flatten().or_else(|| folder_of(&module)) else { continue };
        let own = map.prefix.is_empty();
        for file in sg_files(&folder) {
            sources.add(shown(&file), &module, &read(&file)?, every_module && own, map.clone());
        }
    }
    sources.add_std();
    Ok(sources)
}

/// The packages the program uses: those its `sg.project` names, each from the folder its
/// `local` line gives or from the cache at the version `sg.lock` holds, and those they use in
/// turn, which the program need not name (choice 1576). The lock is only read here: one that
/// does not fit `sg.project` stops the program, and `sg lock` writes it again (choice 1580).
fn packages(project: &Project, base: &Path) -> Result<Vec<Package>, String> {
    struct Found {
        name: String,
        address: String,
        version: project::Version,
        from_lock: bool,
        named: bool,
        allows_unsafe: bool,
        folder: PathBuf,
        own: Project,
    }
    let at = |line: usize| format!("{}:{line}", project.path.display());
    let project_folder = project.path.parent().map(Path::to_path_buf).unwrap_or_default();
    let lock = fetch::read_lock(&project_folder)?;
    let lock_path = project_folder.join(fetch::LOCK);
    let locked = |address: &str, major: u64| lock.iter().find(|entry| entry.address == address && entry.version.major == major);
    // The `sg.project` of a package, which says its address (choice 1565).
    let own_project = |folder: &Path, address: &str, place: &str| -> Result<Project, String> {
        let Some(own) = project::read(folder)? else {
            return Err(format!("{place}: {} has no `{}`, which every package has, with its `package` line", folder.display(), project::FILE));
        };
        match &own.package {
            Some(said) if *said == address => Ok(own),
            Some(said) => Err(format!("{place}: the package in {} is {said}, and the line asks for {address}", folder.display())),
            None => Err(format!("{}: the `{}` of a package says where it is fetched from: `package {address}`", own.path.display(), project::FILE)),
        }
    };
    let mut found: Vec<Found> = Vec::new();
    for require in &project.requires {
        if holds_modules(base, &require.name) {
            return Err(format!("{}: the folder `{}` beside the program has the name of the package `{}`: one of them is named otherwise", at(require.line), require.name, require.name));
        }
        let (folder, version, from_lock, place) = match project.locals.iter().find(|local| local.name == require.name) {
            Some(local) => {
                let folder = cleaned(&project_folder.join(&local.folder));
                if !folder.is_dir() {
                    return Err(format!("{}: there is no folder {} for the package `{}`", at(local.line), folder.display(), require.name));
                }
                (folder, require.version, false, at(local.line))
            }
            None => {
                let Some(entry) = locked(&require.address, require.version.major) else {
                    return Err(format!("{}: {} has no {} {}: {}", at(require.line), lock_path.display(), require.address, require.version, fetch::LOCK_AGAIN));
                };
                if entry.version < require.version {
                    return Err(format!("{}: {} has {} {}, older than the {} the line asks for: {}", at(require.line), lock_path.display(), entry.address, entry.version, require.version, fetch::LOCK_AGAIN));
                }
                let (_, folder) = fetch::fetched(&entry.address, entry.version, Some(&entry.sum)).map_err(|error| format!("{}: {error}", at(require.line)))?;
                (folder, entry.version, true, at(require.line))
            }
        };
        let own = own_project(&folder, &require.address, &place)?;
        found.push(Found { name: require.name.clone(), address: require.address.clone(), version, from_lock, named: true, allows_unsafe: require.allows_unsafe, folder, own });
    }
    // The packages they use, which the program does not name: from the lock, under a name no
    // file can reach, as `utf_1`.
    let mut taken: Vec<String> = found.iter().map(|package| package.name.clone()).collect();
    let mut next = 0;
    while next < found.len() {
        let user = found[next].name.clone();
        let own_path = found[next].own.path.clone();
        let wanted: Vec<(String, project::Version, usize)> = found[next].own.requires.iter().map(|needed| (needed.address.clone(), needed.version, needed.line)).collect();
        for (address, version, line) in wanted {
            let place = format!("{}:{line}", own_path.display());
            let too_old = |have: project::Version| format!("{place}: the package `{user}` uses {address} {version}, and {} has {have}: {}", lock_path.display(), fetch::LOCK_AGAIN);
            if let Some(other) = found.iter().find(|other| other.address == address && other.version.major == version.major) {
                if other.from_lock && other.version < version {
                    return Err(too_old(other.version));
                }
                continue;
            }
            let Some(entry) = locked(&address, version.major) else {
                return Err(format!("{place}: the package `{user}` uses {address} {version}, which {} does not have: {}", lock_path.display(), fetch::LOCK_AGAIN));
            };
            if entry.version < version {
                return Err(too_old(entry.version));
            }
            let (_, folder) = fetch::fetched(&entry.address, entry.version, Some(&entry.sum)).map_err(|error| format!("{place}: {error}"))?;
            let own = own_project(&folder, &address, &place)?;
            let stem = format!("{}_{}", project::suggested(address.rsplit('/').next().unwrap_or(&address)), version.major);
            let mut name = stem.clone();
            let mut count = 2;
            while taken.contains(&name) || holds_modules(base, &name) {
                name = format!("{stem}_{count}");
                count += 1;
            }
            taken.push(name.clone());
            found.push(Found { name, address, version: entry.version, from_lock: true, named: false, allows_unsafe: false, folder, own });
        }
        next += 1;
    }
    for entry in &lock {
        if !found.iter().any(|package| package.address == entry.address && package.version.major == entry.version.major) {
            return Err(format!("{}: {} {} is used by nothing: {}", lock_path.display(), entry.address, entry.version, fetch::LOCK_AGAIN));
        }
    }
    let mut packages = Vec::new();
    for package in &found {
        let mut names = Vec::new();
        for needed in &package.own.requires {
            let given = found.iter().find(|other| other.address == needed.address && other.version.major == needed.version.major).expect("every package used is found");
            if holds_modules(&package.folder, &needed.name) {
                return Err(format!("{}:{}: the folder `{}` of the package has the name of the package it uses: one of them is named otherwise", package.own.path.display(), needed.line, needed.name));
            }
            names.push((needed.name.clone(), given.name.clone()));
        }
        let map = Arc::new(ModuleMap { prefix: package.name.clone(), packages: names, every_package: Arc::default() });
        packages.push(Package {
            name: package.name.clone(),
            address: package.address.clone(),
            version: package.version,
            folder: package.folder.clone(),
            named: package.named,
            allows_unsafe: package.allows_unsafe,
            map,
        });
    }
    Ok(packages)
}

/// A folder in `base` with source files in it or below it, named as a type of the language.
fn folder_named_as_a_type(base: &Path) -> Option<PathBuf> {
    let listed = if base.as_os_str().is_empty() { Path::new(".") } else { base };
    let mut found: Vec<PathBuf> = std::fs::read_dir(listed).ok()?.flatten().filter(|entry| entry.file_name().to_str().is_some_and(crate::check::is_language_type)).map(|entry| base.join(entry.file_name())).collect();
    found.sort();
    found.into_iter().find(|folder| {
        if !folder.is_dir() || is_project(folder) {
            return false;
        }
        let mut below = Vec::new();
        modules_under(folder, "x", &mut below);
        !sg_files(folder).is_empty() || !below.is_empty()
    })
}

/// Whether the folder `name` under `base` holds modules: source files, in it or below it.
fn holds_modules(base: &Path, name: &str) -> bool {
    if !spelled_as(base, name) {
        return false;
    }
    let folder = base.join(name);
    let mut below = Vec::new();
    modules_under(&folder, name, &mut below);
    !sg_files(&folder).is_empty() || !below.is_empty()
}

/// A path with `.` and the `..` that follow a name taken away, as Go's `filepath.Clean` does:
/// `shop/../lib/xml` is `lib/xml`, so the files of a package are named as the reader sees them.
pub(crate) fn cleaned(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out: Vec<Component> = Vec::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir if matches!(out.last(), Some(Component::Normal(_))) => {
                out.pop();
            }
            Component::ParentDir if matches!(out.last(), Some(Component::RootDir | Component::Prefix(_))) => {}
            other => out.push(other),
        }
    }
    out.iter().collect()
}
