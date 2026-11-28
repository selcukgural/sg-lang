//! The language server from outside (choice 1375): a `sg lsp` process, talked to in JSON-RPC as
//! an editor talks to it. It shows the first error, takes it away once the text is right, reads
//! what the editor has not saved, and gives fixes whose code compiles (choice 1371).

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use sg::json::{self, Json};

struct Client {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next_id: u64,
}

impl Client {
    fn start() -> Client {
        Client::start_with(&[])
    }

    /// The server with these variables of the environment.
    fn start_with(variables: &[(&str, &Path)]) -> Client {
        Client::start_as(variables, &["lsp"])
    }

    /// The server started with these words.
    fn start_as(variables: &[(&str, &Path)], words: &[&str]) -> Client {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sg"));
        for (name, value) in variables {
            command.env(name, value);
        }
        let mut child = command.args(words).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut client = Client { child, input, output, next_id: 1 };
        let reply = client.request("initialize", Json::obj(vec![]));
        assert_eq!(reply.get("result").get("capabilities").get("positionEncoding").as_str(), Some("utf-16"));
        client.notify("initialized", Json::obj(vec![]));
        client
    }

    fn write(&mut self, message: Json) {
        let body = message.to_string();
        write!(self.input, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        self.input.flush().unwrap();
    }

    fn read(&mut self) -> Json {
        let mut length = 0;
        loop {
            let mut line = String::new();
            assert!(self.output.read_line(&mut line).unwrap() > 0, "the server ended");
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length: ") {
                length = value.parse().unwrap();
            }
        }
        let mut body = vec![0; length];
        self.output.read_exact(&mut body).unwrap();
        json::parse(std::str::from_utf8(&body).unwrap()).unwrap()
    }

    fn notify(&mut self, method: &str, params: Json) {
        self.write(Json::obj(vec![("jsonrpc", Json::str("2.0")), ("method", Json::str(method)), ("params", params)]));
    }

    /// Sends a request and gives its reply, passing over the notifications that come first.
    fn request(&mut self, method: &str, params: Json) -> Json {
        let id = self.next_id;
        self.next_id += 1;
        self.write(Json::obj(vec![("jsonrpc", Json::str("2.0")), ("id", Json::num(id as f64)), ("method", Json::str(method)), ("params", params)]));
        loop {
            let message = self.read();
            if message.get("id").as_u64() == Some(id) {
                return message;
            }
        }
    }

    /// The diagnostics published next for `uri`.
    fn diagnostics(&mut self, uri: &str) -> Vec<Json> {
        loop {
            let message = self.read();
            if message.get("method").as_str() == Some("textDocument/publishDiagnostics") && message.get("params").get("uri").as_str() == Some(uri) {
                return message.get("params").get("diagnostics").as_array().to_vec();
            }
        }
    }

    fn open(&mut self, uri: &str, text: &str) {
        let doc = Json::obj(vec![("uri", Json::str(uri)), ("languageId", Json::str("sg")), ("version", Json::num(1)), ("text", Json::str(text))]);
        self.notify("textDocument/didOpen", Json::obj(vec![("textDocument", doc)]));
    }

    fn change(&mut self, uri: &str, text: &str) {
        let doc = Json::obj(vec![("uri", Json::str(uri)), ("version", Json::num(2))]);
        let changes = Json::Arr(vec![Json::obj(vec![("text", Json::str(text))])]);
        self.notify("textDocument/didChange", Json::obj(vec![("textDocument", doc), ("contentChanges", changes)]));
    }

    /// Asks for the fixes on the lines of `diagnostic` and gives the one edit of the first.
    fn fix(&mut self, uri: &str, diagnostic: &Json) -> (Json, String) {
        let params = Json::obj(vec![
            ("textDocument", Json::obj(vec![("uri", Json::str(uri))])),
            ("range", diagnostic.get("range").clone()),
            ("context", Json::obj(vec![("diagnostics", Json::Arr(vec![diagnostic.clone()]))])),
        ]);
        let reply = self.request("textDocument/codeAction", params);
        let action = reply.get("result").as_array().first().cloned().expect("a fix");
        let Json::Obj(changes) = action.get("edit").get("changes").clone() else { panic!("no changes") };
        let (edited, edits) = &changes[0];
        let edit = edits.as_array()[0].clone();
        (edit, edited.clone())
    }

    /// Asks the server to end, as an editor does, and gives its exit code.
    fn finish(mut self) -> i32 {
        let reply = self.request("shutdown", Json::Null);
        assert!(reply.get("error").is_null());
        self.notify("exit", Json::Null);
        self.child.wait().unwrap().code().unwrap()
    }
}

/// A folder of its own for a test.
fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("lsp").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(&dir).unwrap()
}

/// The byte of `text` at a position of the protocol, whose column counts UTF-16 code units.
fn offset(text: &str, at: &Json) -> usize {
    let (line, column) = (at.get("line").as_u64().unwrap() as usize, at.get("character").as_u64().unwrap() as usize);
    let start: usize = text.split_inclusive('\n').take(line).map(str::len).sum();
    let mut units = 0;
    for (at, c) in text[start..].char_indices() {
        if units >= column {
            return start + at;
        }
        units += c.len_utf16();
    }
    text.len()
}

fn apply(text: &str, edit: &Json) -> String {
    let range = edit.get("range");
    let (start, end) = (offset(text, range.get("start")), offset(text, range.get("end")));
    format!("{}{}{}", &text[..start], edit.get("newText").as_str().unwrap(), &text[end..])
}

fn checks(path: &Path) -> Result<(), String> {
    let sources = sg::load(path, false)?;
    sg::check_sources(&sources).map_err(|diag| diag.msg)
}

#[test]
fn shows_the_first_error_and_takes_it_away() {
    let dir = folder("first_error");
    let path = dir.join("count.sg");
    std::fs::write(&path, "void main() {\n}\n").unwrap();
    let uri = sg::lsp::path_to_uri(&path);
    let mut client = Client::start();
    // A character of two UTF-16 units before the error: the column counts them as two.
    client.open(&uri, "void main() {\n    var 😀name = 1\n}\n");
    let found = client.diagnostics(&uri);
    assert_eq!(found.len(), 1, "{found:?}");
    client.open(&uri, "void main() {\n    print(\"😀\" + undefinedName)\n}\n");
    let found = client.diagnostics(&uri);
    assert_eq!(found.len(), 1);
    let message = found[0].get("message").as_str().unwrap();
    assert!(message.contains("undefinedName"), "{message}");
    let start = found[0].get("range").get("start");
    assert_eq!((start.get("line").as_u64(), start.get("character").as_u64()), (Some(1), Some(17)));
    client.change(&uri, "void main() {\n    print(1)\n}\n");
    assert!(client.diagnostics(&uri).is_empty());
    assert_eq!(client.finish(), 0);
}

#[test]
fn reads_the_text_the_editor_has_not_saved() {
    let dir = folder("unsaved");
    std::fs::create_dir_all(dir.join("tools")).unwrap();
    std::fs::write(dir.join("tools").join("math.sg"), "public int twice(int x) {\n    return x * 2\n}\n").unwrap();
    let main = dir.join("main.sg");
    std::fs::write(&main, "use tools\n\nvoid main() {\n    print(twice(2))\n}\n").unwrap();
    let (main_uri, module_uri) = (sg::lsp::path_to_uri(&main), sg::lsp::path_to_uri(&dir.join("tools").join("math.sg")));
    let mut client = Client::start();
    client.open(&main_uri, "use tools\n\nvoid main() {\n    print(thrice(2))\n}\n");
    assert_eq!(client.diagnostics(&main_uri).len(), 1);
    // The module, changed in the editor and not saved, now has the function.
    client.open(&module_uri, "public int twice(int x) {\n    return x * 2\n}\n\npublic int thrice(int x) {\n    return x * 3\n}\n");
    loop {
        if client.diagnostics(&main_uri).is_empty() {
            break;
        }
    }
    // An error in the module is shown in the module's file.
    client.change(&module_uri, "public int twice(int x) {\n    return x * 2\n}\n\npublic int thrice(int x) {\n    return y * 3\n}\n");
    // What was published for the module before the change may come first.
    let found = loop {
        let found = client.diagnostics(&module_uri);
        if !found.is_empty() {
            break found;
        }
    };
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].get("range").get("start").get("line").as_u64(), Some(5));
    assert_eq!(client.finish(), 0);
}

#[test]
fn fixes_give_code_that_compiles() {
    let programs = [
        ("cast", "void main() {\n    long big = 5\n    int small = big + 1\n    print(small)\n}\n"),
        ("clone", "string own(read string name) {\n    return name\n}\n\nvoid main() {\n    print(own(\"ada\"))\n}\n"),
        ("dropped", "int answer() {\n    return 42\n}\n\nvoid main() {\n    answer()\n}\n"),
    ];
    let mut client = Client::start();
    for (name, text) in programs {
        let dir = folder(&format!("fix_{name}"));
        let path = dir.join("prog.sg");
        let uri = sg::lsp::path_to_uri(&path);
        client.open(&uri, text);
        let found = client.diagnostics(&uri);
        assert_eq!(found.len(), 1, "{name}: {found:?}");
        let (edit, edited) = client.fix(&uri, &found[0]);
        assert_eq!(edited, uri);
        let fixed = apply(text, &edit);
        std::fs::write(&path, &fixed).unwrap();
        assert_eq!(checks(&path), Ok(()), "{name}: the fixed program\n{fixed}");
    }
    // The missing `use`, put at the top of the file after its comments.
    let dir = folder("fix_use");
    std::fs::create_dir_all(dir.join("tools")).unwrap();
    std::fs::write(dir.join("tools").join("math.sg"), "public int twice(int x) {\n    return x * 2\n}\n").unwrap();
    let path = dir.join("main.sg");
    let text = "// Twice two.\n\nvoid main() {\n    print(twice(2))\n}\n";
    std::fs::write(&path, text).unwrap();
    let uri = sg::lsp::path_to_uri(&path);
    client.open(&uri, text);
    let found = client.diagnostics(&uri);
    assert_eq!(found.len(), 1);
    let (edit, _) = client.fix(&uri, &found[0]);
    let fixed = apply(text, &edit);
    assert!(fixed.starts_with("// Twice two.\n\nuse tools\n"), "{fixed}");
    std::fs::write(&path, &fixed).unwrap();
    assert_eq!(checks(&path), Ok(()));
    assert_eq!(client.finish(), 0);
}

#[test]
fn answers_what_it_does_not_serve_and_ends_as_told() {
    let mut client = Client::start();
    let reply = client.request("textDocument/hover", Json::obj(vec![]));
    assert_eq!(reply.get("error").get("code"), &Json::num(-32601));
    assert_eq!(client.finish(), 0);
    // `exit` without `shutdown` before it is an exit with code 1, as the protocol has it.
    let mut client = Client::start();
    client.notify("exit", Json::Null);
    assert_eq!(client.child.wait().unwrap().code(), Some(1));
    // `--stdio`, which clients add, names the way it talks already (choice 2237); another word is
    // a usage error.
    let client = Client::start_as(&[], &["lsp", "--stdio"]);
    assert_eq!(client.finish(), 0);
    let refused = Command::new(env!("CARGO_BIN_EXE_sg")).args(["lsp", "--stdio", "--stdio"]).stdin(Stdio::null()).output().unwrap();
    assert_eq!(refused.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&refused.stderr).starts_with("usage: sg "));
}

/// The server fetches no package (choice 1591): opening a file of a folder whose `sg.lock` names a
/// package the cache has not is no reason to reach the network, and the message says what does.
#[test]
fn fetches_no_package() {
    let dir = folder("fetches_no_package");
    let (cache, mirror) = (dir.join("cache"), dir.join("mirror"));
    std::fs::create_dir_all(mirror.join("example.com/bob/utf")).unwrap();
    std::fs::write(dir.join("sg.project"), "require utf example.com/bob/utf 1.0.0\n").unwrap();
    std::fs::write(dir.join("sg.lock"), format!("example.com/bob/utf 1.0.0 sha256-{}\n", "0".repeat(64))).unwrap();
    let path = dir.join("main.sg");
    let text = "use utf\n\nvoid main() {\n    print(utf.one())\n}\n";
    std::fs::write(&path, text).unwrap();
    let uri = sg::lsp::path_to_uri(&path);
    let mut client = Client::start_with(&[("SG_CACHE", &cache), ("SG_MIRROR", &mirror)]);
    client.open(&uri, text);
    let found = client.diagnostics(&uri);
    let message = found.first().and_then(|diagnostic| diagnostic.get("message").as_str()).unwrap_or_default().to_string();
    assert!(message.contains("example.com/bob/utf 1.0.0 is not in the cache, and the language server fetches nothing"), "{found:?}");
    assert!(!cache.exists(), "nothing is made in the cache");
    assert_eq!(client.finish(), 0);
}

/// A message in any letters is shown, and the server goes on checking (1596): a word of more
/// than one byte before a backtick stopped the thread that checks, and what it showed last
/// stayed for ever.
#[test]
fn shows_a_message_in_any_letters() {
    let dir = folder("any_letters");
    let path = dir.join("main.sg");
    std::fs::write(&path, "void main() {\n}\n").unwrap();
    let uri = sg::lsp::path_to_uri(&path);
    let mut client = Client::start();
    client.open(&uri, "const byte[] Logo = embed(\"a`ş `.bin\")\n\nvoid main() {\n    print(Logo.length())\n}\n");
    let found = client.diagnostics(&uri);
    let message = found.first().and_then(|diagnostic| diagnostic.get("message").as_str()).unwrap_or_default().to_string();
    assert!(message.contains("there is no file `a`ş `.bin`"), "{found:?}");
    client.change(&uri, "void main() {\n    print(1)\n}\n");
    assert!(client.diagnostics(&uri).is_empty());
    assert_eq!(client.finish(), 0);
}

/// Texts nested deeper than the parser takes are an error, not the end of the server (1596).
#[test]
fn refuses_texts_nested_too_deep() {
    let dir = folder("nested_too_deep");
    let path = dir.join("main.sg");
    std::fs::write(&path, "void main() {\n}\n").unwrap();
    let uri = sg::lsp::path_to_uri(&path);
    let mut body = "1".to_string();
    for _ in 0..8000 {
        body = format!("$\"{{{body}}}\"");
    }
    let mut client = Client::start();
    client.open(&uri, &format!("void main() {{\n    print({body})\n}}\n"));
    let found = client.diagnostics(&uri);
    let message = found.first().and_then(|diagnostic| diagnostic.get("message").as_str()).unwrap_or_default().to_string();
    assert!(message.contains("nested more than 1000 deep"), "{message}");
    assert_eq!(client.finish(), 0);
}

/// A folder whose links lead back into it is walked once (1596): the server checked it for ever,
/// published nothing and did not end when told.
#[cfg(unix)]
#[test]
fn checks_a_folder_whose_links_go_round() {
    let dir = folder("links_go_round");
    for name in ["a", "b", "c"] {
        std::os::unix::fs::symlink(".", dir.join(name)).unwrap();
    }
    let path = dir.join("main.sg");
    let text = "void main() {\n    var x = Missing()\n}\n";
    std::fs::write(&path, text).unwrap();
    let uri = sg::lsp::path_to_uri(&path);
    let mut client = Client::start();
    client.open(&uri, text);
    let found = client.diagnostics(&uri);
    let message = found.first().and_then(|diagnostic| diagnostic.get("message").as_str()).unwrap_or_default().to_string();
    assert!(message.contains("there is no function named `Missing`"), "{message}");
    assert_eq!(client.finish(), 0);
}

/// A line of a header is read only as far as its limit (1596): one without an end was kept
/// whole in memory, however long, and the server waited for its end.
#[test]
fn refuses_a_header_line_without_an_end() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sg")).arg("lsp").stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap();
    let mut input = child.stdin.take().unwrap();
    // More than the limit, and the input left open: the server ends without waiting for more.
    let _ = input.write_all(&vec![b'A'; 100_000]);
    let _ = input.flush();
    let started = std::time::Instant::now();
    let code = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status.code();
        }
        if started.elapsed() > std::time::Duration::from_secs(20) {
            let _ = child.kill();
            panic!("the server waited for the end of the line");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    drop(input);
    let mut err = String::new();
    child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
    assert_eq!(code, Some(1));
    assert!(err.contains("a line of a header longer than 8192 bytes"), "{err}");
}

#[test]
fn shows_a_use_that_brings_no_name_dimmed() {
    // Choice 2256: with no error, each `use` that brings no name is a hint with the tag
    // `Unnecessary`, which the editor shows dimmed; a name written with its module in front
    // needs no `use`.
    let dir = folder("unused_use");
    for (module, name) in [("tools", "twice"), ("other", "thrice"), ("third", "once")] {
        std::fs::create_dir_all(dir.join(module)).unwrap();
        std::fs::write(dir.join(module).join(format!("{module}.sg")), format!("public int {name}(int x) {{\n    return x\n}}\n")).unwrap();
    }
    let main = dir.join("main.sg");
    let text = "use tools\nuse other\nuse third\n\nvoid main() {\n    print(twice(2))\n    print(third.once(1))\n}\n";
    std::fs::write(&main, text).unwrap();
    let uri = sg::lsp::path_to_uri(&main);
    let mut client = Client::start();
    client.open(&uri, text);
    let found = client.diagnostics(&uri);
    let lines: Vec<u64> = found.iter().map(|shown| shown.get("range").get("start").get("line").as_u64().unwrap()).collect();
    assert_eq!(lines, [1, 2], "{found:?}");
    for shown in &found {
        assert_eq!(shown.get("severity").as_u64(), Some(4));
        assert_eq!(shown.get("tags").as_array().iter().map(|tag| tag.as_u64()).collect::<Vec<_>>(), [Some(1)]);
    }
    // While there is an error, it alone is shown: which names are brought is not known then.
    client.change(&uri, "use tools\nuse other\nuse third\n\nvoid main() {\n    print(twice(missing))\n}\n");
    let found = client.diagnostics(&uri);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].get("severity").as_u64(), Some(1));
    // Every `use` brings a name: nothing is shown.
    client.change(&uri, "use tools\nuse other\nuse third\n\nvoid main() {\n    print(twice(2) + thrice(3) + once(1))\n}\n");
    assert!(client.diagnostics(&uri).is_empty());
    assert_eq!(client.finish(), 0);
}
