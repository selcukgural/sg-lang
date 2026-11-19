//! The language server, `sg lsp` (choices 1369-1376): an editor starts it and talks to it on its
//! input and output in the Language Server Protocol. It shows the first error of the program a
//! file belongs to as the file is written, and applies a hint that has a fix with one click.
//!
//! A file belongs to the program `sg` would read it in: the nearest folder above it with a
//! `main.sg` is a program of a folder, and otherwise the file is a program of its own, its
//! modules in the folders beside it. What the editor holds and has not saved is read instead of
//! the disk. The program is checked 300 ms after the last change, on a thread of its own, and
//! only checked: it is not run.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::diag::{Diag, Span};
use crate::json::{self, Json};
use crate::source::Sources;

/// How long the text must stay as it is before it is checked.
const QUIET: Duration = Duration::from_millis(300);

/// The most bytes one message may have.
const MAX_MESSAGE: usize = 64 << 20;

/// How many folders up a `main.sg` is looked for.
const MAX_UP: usize = 32;

struct Doc {
    uri: String,
    text: String,
}

/// A fix the editor may ask for: the range of its diagnostic, the diagnostic, and the edit.
struct Fixable {
    range: (u64, u64, u64, u64),
    diagnostic: Json,
    title: String,
    edit: Json,
}

#[derive(Default)]
struct State {
    docs: HashMap<PathBuf, Doc>,
    /// The fixes of each file whose diagnostics were last published, by its URI.
    fixes: HashMap<String, Vec<Fixable>>,
    /// The files that have diagnostics in the editor now.
    shown: HashSet<String>,
}

type Out = Arc<Mutex<Box<dyn Write + Send>>>;

fn send(out: &Out, message: &Json) {
    let body = message.to_string();
    let mut out = out.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = write!(out, "Content-Length: {}\r\n\r\n{body}", body.len());
    let _ = out.flush();
}

/// Reads one message: its headers, then as many bytes as `Content-Length` says. `None` at the end
/// of the input.
fn read_message(input: &mut impl BufRead) -> Result<Option<String>, String> {
    let mut length = None;
    loop {
        let mut line = String::new();
        match input.read_line(&mut line) {
            Ok(0) => return Ok(None),
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse::<usize>().ok();
            }
        }
    }
    let length = length.ok_or("a message without a Content-Length")?;
    if length > MAX_MESSAGE {
        return Err(format!("a message of {length} bytes, more than {MAX_MESSAGE}"));
    }
    let mut body = vec![0; length];
    std::io::Read::read_exact(input, &mut body).map_err(|error| error.to_string())?;
    String::from_utf8(body).map(Some).map_err(|_| "a message that is not UTF-8".to_string())
}

/// Serves until the editor says `exit` or closes the input. The exit code: 0 after `shutdown`.
pub fn serve(mut input: impl BufRead, output: impl Write + Send + 'static) -> i32 {
    let out: Out = Arc::new(Mutex::new(Box::new(output)));
    let state = Arc::new(Mutex::new(State::default()));
    let (changed, changes) = mpsc::channel::<()>();
    let checker = {
        let (state, out) = (state.clone(), out.clone());
        std::thread::spawn(move || {
            while changes.recv().is_ok() {
                // Changes that come close together are checked once.
                loop {
                    match changes.recv_timeout(QUIET) {
                        Ok(()) => continue,
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
                check_open(&state, &out);
            }
        })
    };
    let mut shut_down = false;
    let code = loop {
        let message = match read_message(&mut input) {
            Ok(Some(message)) => message,
            Ok(None) => break if shut_down { 0 } else { 1 },
            Err(error) => {
                eprintln!("sg lsp: {error}");
                break 1;
            }
        };
        let Ok(message) = json::parse(&message) else {
            send(&out, &error_reply(Json::Null, -32700, "the message is not JSON"));
            continue;
        };
        let method = message.get("method").as_str().unwrap_or("").to_string();
        let id = message.get("id").clone();
        let params = message.get("params");
        let is_request = !id.is_null();
        match method.as_str() {
            "initialize" => send(&out, &reply(id, capabilities())),
            "shutdown" => {
                shut_down = true;
                send(&out, &reply(id, Json::Null));
            }
            "exit" => break if shut_down { 0 } else { 1 },
            "textDocument/didOpen" => {
                let doc = params.get("textDocument");
                if let (Some(uri), Some(text)) = (doc.get("uri").as_str(), doc.get("text").as_str()) {
                    if let Some(path) = uri_to_path(uri) {
                        lock(&state).docs.insert(path, Doc { uri: uri.to_string(), text: text.to_string() });
                        let _ = changed.send(());
                    }
                }
            }
            "textDocument/didChange" => {
                let uri = params.get("textDocument").get("uri").as_str().unwrap_or("");
                // The whole text comes with each change (`textDocumentSync` 1): the last is kept.
                let text = params.get("contentChanges").as_array().iter().rev().find_map(|change| change.get("text").as_str());
                if let (Some(path), Some(text)) = (uri_to_path(uri), text) {
                    if let Some(doc) = lock(&state).docs.get_mut(&path) {
                        doc.text = text.to_string();
                    }
                    let _ = changed.send(());
                }
            }
            "textDocument/didClose" => {
                if let Some(path) = params.get("textDocument").get("uri").as_str().and_then(uri_to_path) {
                    lock(&state).docs.remove(&path);
                    let _ = changed.send(());
                }
            }
            "textDocument/didSave" => {
                let _ = changed.send(());
            }
            "textDocument/codeAction" => send(&out, &reply(id, code_actions(&state, params))),
            _ if is_request => send(&out, &error_reply(id, -32601, &format!("`{method}` is not served"))),
            _ => {}
        }
    };
    drop(changed);
    let _ = checker.join();
    code
}

fn lock(state: &Arc<Mutex<State>>) -> std::sync::MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn reply(id: Json, result: Json) -> Json {
    Json::obj(vec![("jsonrpc", Json::str("2.0")), ("id", id), ("result", result)])
}

fn error_reply(id: Json, code: i32, message: &str) -> Json {
    Json::obj(vec![("jsonrpc", Json::str("2.0")), ("id", id), ("error", Json::obj(vec![("code", Json::num(code)), ("message", Json::str(message))]))])
}

fn capabilities() -> Json {
    Json::obj(vec![
        (
            "capabilities",
            Json::obj(vec![
                ("positionEncoding", Json::str("utf-16")),
                ("textDocumentSync", Json::obj(vec![("openClose", Json::Bool(true)), ("change", Json::num(1)), ("save", Json::Bool(true))])),
                ("codeActionProvider", Json::obj(vec![("codeActionKinds", Json::Arr(vec![Json::str("quickfix")]))])),
            ]),
        ),
        ("serverInfo", Json::obj(vec![("name", Json::str("sg")), ("version", Json::str(env!("CARGO_PKG_VERSION")))])),
    ])
}

/// The folder of the program a file belongs to, or the file itself.
fn program_of(file: &Path) -> PathBuf {
    let mut folder = file.parent();
    for _ in 0..MAX_UP {
        let Some(dir) = folder else { break };
        if dir.join("main.sg").is_file() {
            return dir.to_path_buf();
        }
        folder = dir.parent();
    }
    file.to_path_buf()
}

/// Checks the program of every open file and publishes what was found.
fn check_open(state: &Arc<Mutex<State>>, out: &Out) {
    let (open, uris): (HashMap<PathBuf, String>, HashMap<PathBuf, String>) = {
        let state = lock(state);
        (state.docs.iter().map(|(path, doc)| (path.clone(), doc.text.clone())).collect(), state.docs.iter().map(|(path, doc)| (path.clone(), doc.uri.clone())).collect())
    };
    let mut programs: Vec<(PathBuf, Vec<PathBuf>)> = Vec::new();
    for path in open.keys() {
        let root = program_of(path);
        match programs.iter_mut().find(|(seen, _)| *seen == root) {
            Some((_, files)) => files.push(path.clone()),
            None => programs.push((root, vec![path.clone()])),
        }
    }
    let uri_of = |path: &Path| uris.get(path).cloned().unwrap_or_else(|| path_to_uri(path));
    let mut found: HashMap<String, Vec<(Json, Option<Fixable>)>> = HashMap::new();
    let mut checked: Vec<String> = Vec::new();
    for (root, files) in &programs {
        let mut first: Vec<&PathBuf> = files.iter().collect();
        first.sort();
        let fallback = uri_of(first[0]);
        checked.extend(files.iter().map(|file| uri_of(file)));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match crate::load_over(root, false, &open) {
            Ok(sources) => crate::check_sources(&sources).err().map(|diag| (Some(sources), diag)),
            Err(message) => Some((None, Diag::error(Span::new(0, 0), message))),
        }));
        let (sources, diag) = match outcome {
            Ok(None) => continue,
            Ok(Some(found)) => found,
            Err(_) => (None, Diag::error(Span::new(0, 0), "the checker stopped: this is a mistake in sg").note("please report it, with the program it was checking")),
        };
        let (uri, diagnostic, fix) = diagnostic_of(&diag, sources.as_ref(), &fallback, &uri_of);
        found.entry(uri).or_default().push((diagnostic, fix));
    }
    let mut state = lock(state);
    let mut to_publish: Vec<String> = state.shown.iter().cloned().collect();
    to_publish.extend(checked);
    to_publish.extend(found.keys().cloned());
    to_publish.sort();
    to_publish.dedup();
    state.shown.clear();
    for uri in to_publish {
        let entries = found.remove(&uri).unwrap_or_default();
        let diagnostics: Vec<Json> = entries.iter().map(|(diagnostic, _)| diagnostic.clone()).collect();
        if !diagnostics.is_empty() {
            state.shown.insert(uri.clone());
        }
        state.fixes.insert(uri.clone(), entries.into_iter().filter_map(|(_, fix)| fix).collect());
        let params = Json::obj(vec![("uri", Json::str(uri)), ("diagnostics", Json::Arr(diagnostics))]);
        send(out, &Json::obj(vec![("jsonrpc", Json::str("2.0")), ("method", Json::str("textDocument/publishDiagnostics")), ("params", params)]));
    }
}

/// Where a place of the program is: the URI of its file and its range there. A place in the
/// language's own module, or in no file, is shown at the start of `fallback`.
fn place_of(span: Span, sources: Option<&Sources>, fallback: &str, uri_of: &dyn Fn(&Path) -> String) -> (String, (u64, u64, u64, u64)) {
    let Some(sources) = sources else { return (fallback.to_string(), (0, 0, 0, 0)) };
    let file = sources.file_at(span.start);
    if file.path.starts_with("<built-in>") {
        return (fallback.to_string(), (0, 0, 0, 0));
    }
    let text = &sources.text[file.start..file.end];
    let start = span.start.clamp(file.start, file.end) - file.start;
    let end = span.end.clamp(file.start, file.end) - file.start;
    let (line, character) = position(text, start);
    let (end_line, end_character) = position(text, end.max(start));
    (uri_of(Path::new(&file.path)), (line, character, end_line, end_character))
}

fn range_json((line, character, end_line, end_character): (u64, u64, u64, u64)) -> Json {
    let at = |line: u64, character: u64| Json::obj(vec![("line", Json::num(line as f64)), ("character", Json::num(character as f64))]);
    Json::obj(vec![("start", at(line, character)), ("end", at(end_line, end_character))])
}

/// The line and the column of a byte of the text, the column counted in UTF-16 code units, as
/// the protocol counts it.
pub fn position(text: &str, offset: usize) -> (u64, u64) {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &text[..offset];
    let line = before.matches('\n').count() as u64;
    let line_start = before.rfind('\n').map_or(0, |at| at + 1);
    (line, before[line_start..].encode_utf16().count() as u64)
}

fn diagnostic_of(diag: &Diag, sources: Option<&Sources>, fallback: &str, uri_of: &dyn Fn(&Path) -> String) -> (String, Json, Option<Fixable>) {
    let (uri, range) = place_of(diag.span, sources, fallback, uri_of);
    let mut message = diag.msg.clone();
    for note in &diag.notes {
        message.push('\n');
        message.push_str(note);
    }
    if let Some(hint) = &diag.hint {
        message.push_str("\nhint: ");
        message.push_str(hint);
    }
    let mut members = vec![("range", range_json(range)), ("severity", Json::num(1)), ("source", Json::str("sg")), ("message", Json::str(message))];
    if let Some(related) = &diag.related {
        let (related_uri, related_range) = place_of(related.0, sources, fallback, uri_of);
        let location = Json::obj(vec![("uri", Json::str(related_uri)), ("range", range_json(related_range))]);
        members.push(("relatedInformation", Json::Arr(vec![Json::obj(vec![("location", location), ("message", Json::str(related.1.clone()))])])));
    }
    let diagnostic = Json::obj(members);
    let fix = diag.fix.as_ref().map(|fix| {
        let (edit_uri, edit_range) = place_of(fix.span, sources, fallback, uri_of);
        let edit = Json::obj(vec![("range", range_json(edit_range)), ("newText", Json::str(fix.text.clone()))]);
        let title = match &diag.hint {
            Some(hint) => format!("sg: {hint}"),
            None => "sg: apply the fix".to_string(),
        };
        Fixable { range, diagnostic: diagnostic.clone(), title, edit: Json::obj(vec![("changes", Json::obj(vec![(edit_uri.as_str(), Json::Arr(vec![edit]))]))]) }
    });
    (uri, diagnostic, fix)
}

/// The fixes of the file whose diagnostics lie on the lines the editor asks about.
fn code_actions(state: &Arc<Mutex<State>>, params: &Json) -> Json {
    let uri = params.get("textDocument").get("uri").as_str().unwrap_or("");
    let range = params.get("range");
    let line = |at: &Json| at.get("line").as_u64().unwrap_or(0);
    let (first, last) = (line(range.get("start")), line(range.get("end")));
    let state = lock(state);
    let actions = state.fixes.get(uri).map_or(Vec::new(), |fixes| {
        fixes
            .iter()
            .filter(|fix| fix.range.0 <= last && first <= fix.range.2)
            .map(|fix| {
                Json::obj(vec![
                    ("title", Json::str(fix.title.clone())),
                    ("kind", Json::str("quickfix")),
                    ("diagnostics", Json::Arr(vec![fix.diagnostic.clone()])),
                    ("isPreferred", Json::Bool(true)),
                    ("edit", fix.edit.clone()),
                ])
            })
            .collect()
    });
    Json::Arr(actions)
}

/// The path of a `file:` URI: `file:///home/a%20b/x.sg`, `file:///c%3A/x.sg` on Windows.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' && at + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[at + 1..at + 3]).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            decoded.push(bytes[at]);
            at += 1;
        }
    }
    let path = String::from_utf8(decoded).ok()?;
    // `/c:/x.sg` on Windows is `c:/x.sg`; a host before the path is not taken.
    let path = match cfg!(windows) {
        true => path.strip_prefix('/').filter(|rest| rest.as_bytes().get(1) == Some(&b':')).map(str::to_string).unwrap_or(path),
        false if !path.starts_with('/') => return None,
        false => path,
    };
    Some(PathBuf::from(path))
}

/// The `file:` URI of a path.
pub fn path_to_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut uri = String::from(if text.starts_with('/') { "file://" } else { "file:///" });
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => uri.push(byte as char),
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_columns_in_utf16() {
        let text = "ab\nçö😀x\n";
        assert_eq!(position(text, 0), (0, 0));
        assert_eq!(position(text, 3), (1, 0));
        // `ç` and `ö` are one unit each, `😀` two.
        let x = text.find('x').unwrap();
        assert_eq!(position(text, x), (1, 4));
    }

    #[test]
    fn reads_and_writes_file_uris() {
        let path = uri_to_path("file:///tmp/a%20b/%C3%A7.sg").unwrap();
        assert_eq!(path, PathBuf::from("/tmp/a b/ç.sg"));
        if !cfg!(windows) {
            assert_eq!(path_to_uri(&path), "file:///tmp/a%20b/%C3%A7.sg");
            assert_eq!(uri_to_path("file://host/x.sg"), None);
        }
        assert_eq!(uri_to_path("untitled:Untitled-1"), None);
    }
}
