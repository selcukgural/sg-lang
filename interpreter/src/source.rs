//! The files of a program. A folder is a module: the files in it are one module, named by the
//! path of the folder from the root of the program (`shop/stock` is `shop.stock`). The root
//! itself is the module without a name, where `main` is.
//!
//! All the files are kept as one text, one after the other, so a position in that text says
//! which file it is in.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::diag::{snippet, Span};
use crate::lexer::{lex, Tok};

/// The module of the types the language writes in SG and brings along (`Duration`, `Instant`,
/// `DateTime`): seen from every module with nothing written. No folder of a program has this name.
pub const STD_MODULE: &str = "std";

/// The files of the language's own module, each with the types it brings: a program that names
/// one of them gets the file.
const STD_FILES: &[(&str, &str, &[&str])] = &[
    ("time.sg", include_str!("std/time.sg"), &["Duration", "Instant", "DateTime", "DayOfWeek", "DateError"]),
    ("random.sg", include_str!("std/random.sg"), &["Random", "SecureRandom"]),
    ("files.sg", include_str!("std/files.sg"), &["Directory", "DirectoryEntry", "FileInfo", "FileKind"]),
    ("path.sg", include_str!("std/path.sg"), &["Path"]),
    ("environment.sg", include_str!("std/environment.sg"), &["Environment", "VariableError"]),
    ("process.sg", include_str!("std/process.sg"), &["Command", "Process", "ExitStatus", "ProcessError"]),
    ("net.sg", include_str!("std/net.sg"), &["SocketAddress", "UdpSocket", "Received"]),
    ("signals.sg", include_str!("std/signals.sg"), &["Signal", "Signals", "SignalListener"]),
    ("atomic.sg", include_str!("std/atomic.sg"), &["MemoryOrder"]),
];

/// The functions of the built-in `File` that the language's own module does the work of
/// (`File.info(path)` is `FileInfo`'s): a program that names `File` and one of them gets the file
/// of `FileInfo`.
pub const STD_FILE_CALLS: &[&str] = &["info", "exists"];

/// The methods of the built-in `TcpStream` that the language's own module does the work of
/// (`stream.remoteAddress()` gives a `SocketAddress`): a program that names one gets the file of
/// `SocketAddress`.
pub const STD_STREAM_CALLS: &[&str] = &["remoteAddress", "localAddress"];

/// Every type of the language's own module.
pub fn std_types() -> impl Iterator<Item = &'static str> {
    STD_FILES.iter().flat_map(|(_, _, types)| types.iter().copied())
}

/// The names written in `text` from `start` on.
fn names_in(text: &str, start: usize, named: &mut HashSet<String>) {
    let Ok(toks) = lex(text, Span::new(start, text.len()), false) else { return };
    for tok in toks {
        if let Tok::Ident(name) = tok.tok {
            named.insert(name);
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
}

pub struct Sources {
    pub text: String,
    pub files: Vec<SourceFile>,
    /// The folder the modules are looked for in; `None` for a program that is one text.
    root: Option<PathBuf>,
    /// The folders `-L` names (see `Program::lib_dirs`).
    pub lib_dirs: Vec<PathBuf>,
}

impl Sources {
    /// A program that is one text.
    pub fn single(src: &str, path: &str) -> Sources {
        let file = SourceFile { path: path.to_string(), module: String::new(), start: 0, end: src.len(), tested: true };
        let mut sources = Sources { text: src.to_string(), files: vec![file], root: None, lib_dirs: Vec::new() };
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
        if STD_STREAM_CALLS.iter().any(|name| named.contains(*name)) {
            named.insert("SocketAddress".to_string());
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
                    let start = self.add(format!("<built-in>/std/{file}"), STD_MODULE, text, false);
                    let file = &self.files[start];
                    names_in(&self.text[..file.end], file.start, &mut named);
                }
            }
            if !more {
                break;
            }
        }
    }

    fn add(&mut self, path: String, module: &str, text: &str, tested: bool) -> usize {
        if !self.text.is_empty() {
            self.text.push('\n');
        }
        let start = self.text.len();
        self.text.push_str(text);
        self.files.push(SourceFile { path, module: module.to_string(), start, end: self.text.len(), tested });
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
        let files = self.files.iter().map(|file| SourceFile { path: file.path.clone(), module: file.module.clone(), ..*file }).collect();
        let mut all = Sources { text: self.text.clone(), files, root: None, lib_dirs: self.lib_dirs.clone() };
        for module in modules {
            let folder = module.split('.').fold(root.clone(), |folder, part| folder.join(part));
            for file in sg_files(&folder) {
                let text = std::fs::read_to_string(&file).ok()?;
                all.add(file.to_string_lossy().into_owned(), &module, &text, false);
            }
        }
        Some(all)
    }
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
/// them.
fn modules_under(dir: &Path, prefix: &str, found: &mut Vec<String>) {
    let mut folders: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(entries) => entries.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()).collect(),
        Err(_) => return,
    };
    folders.sort();
    for folder in folders {
        let Some(name) = folder.file_name().and_then(|name| name.to_str()) else { continue };
        if !is_module_name(name) {
            continue;
        }
        let module = if prefix.is_empty() { name.to_string() } else { format!("{prefix}.{name}") };
        if !sg_files(&folder).is_empty() {
            found.push(module.clone());
        }
        modules_under(&folder, &module, found);
    }
}

/// A folder is a module only if its name can be written in the language.
fn is_module_name(name: &str) -> bool {
    if name == STD_MODULE {
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
        let longest = if after_use { chain.len() } else { chain.len() - 1 };
        for length in 1..=longest {
            paths.push(chain[..length].join("."));
        }
    }
    paths
}

/// Reads the program at `path`. A file is a program of one file: it is the root module alone.
/// A folder is a program whose root module is every file in it. The folders next to the root
/// files are the modules; one is read when a file names it. With `every_module` all of them
/// are read and their tests run: that is what the test command does with a folder.
pub fn load(path: &Path, every_module: bool) -> Result<Sources, String> {
    let shown = |path: &Path| path.to_string_lossy().into_owned();
    let read = |path: &Path| std::fs::read_to_string(path).map_err(|error| format!("cannot read {}: {error}", shown(path)));
    let mut sources = Sources { text: String::new(), files: Vec::new(), root: None, lib_dirs: Vec::new() };
    let root = if path.is_dir() {
        let files = sg_files(path);
        if files.is_empty() {
            return Err(format!("there is no `.sg` file in {}", shown(path)));
        }
        for file in files {
            sources.add(shown(&file), "", &read(&file)?, true);
        }
        path.to_path_buf()
    } else {
        sources.add(shown(path), "", &read(path)?, true);
        path.parent().map(Path::to_path_buf).unwrap_or_default()
    };
    let root = if root.as_os_str().is_empty() { PathBuf::from(".") } else { root };
    sources.root = Some(root.clone());

    let mut wanted: Vec<String> = Vec::new();
    if every_module && path.is_dir() {
        modules_under(&root, "", &mut wanted);
    }
    let mut asked: HashMap<String, bool> = HashMap::new();
    let mut loaded: HashSet<String> = HashSet::new();
    let mut scanned = 0;
    loop {
        // What the files read so far name.
        while scanned < sources.files.len() {
            let file = &sources.files[scanned];
            for module in named_paths(&sources.text, Span::new(file.start, file.end)) {
                let exists = *asked.entry(module.clone()).or_insert_with(|| {
                    let folder = module.split('.').fold(root.clone(), |folder, part| folder.join(part));
                    module.split('.').all(is_module_name) && !sg_files(&folder).is_empty()
                });
                if exists {
                    wanted.push(module);
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
        let folder = module.split('.').fold(root.clone(), |folder, part| folder.join(part));
        for file in sg_files(&folder) {
            sources.add(shown(&file), &module, &read(&file)?, every_module);
        }
    }
    sources.add_std();
    Ok(sources)
}
