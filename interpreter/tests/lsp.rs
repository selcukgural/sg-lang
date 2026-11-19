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
        let mut child = Command::new(env!("CARGO_BIN_EXE_sg")).arg("lsp").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
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
}
