//! Names of the system that are not text (choices 1950 to 1957): a word of the command line, a name
//! in a folder and a word given to a started program come into SG whatever bytes they have, and go
//! back to the system as they came, in the interpreter and compiled. `tests/names/names.sg` is the
//! program; it writes the bytes of each name in hex (`Path.systemBytes`), which the test compares
//! with the bytes it made.
//!
//! Linux takes any bytes in a name of a file; macOS takes only UTF-8 there (APFS), so it is tried
//! with the names that are UTF-8 and escaped all the same, those of U+10FF80 to U+10FFFF; Windows
//! takes any units of UTF-16, a half of a pair alone among them, which come as the three bytes of
//! WTF-8. The command line takes any bytes on Linux and macOS.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const LIMIT: std::time::Duration = std::time::Duration::from_secs(300);

fn finished(command: &mut Command) -> Output {
    use std::io::Read;
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap_or_else(|error| panic!("cannot start {:?}: {error}", command.get_program()));
    let take = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let out = take(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err = take(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let start = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{:?} {:?} did not end within {} s", command.get_program(), command.get_args().collect::<Vec<_>>(), LIMIT.as_secs());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    Output { status, stdout: out.join().unwrap(), stderr: err.join().unwrap() }
}

fn text(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("names").join("names.sg")
}

/// A folder of its own in the temporary folder.
fn work(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("sg-names-tests").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The words that start the program: the interpreter with the file, and the compiled program.
fn launchers(dir: &Path) -> Vec<(&'static str, Vec<OsString>)> {
    let sg = OsString::from(env!("CARGO_BIN_EXE_sg"));
    let program = PathBuf::from(sg::cc::program_file(&dir.join("names").to_string_lossy()));
    let built = finished(Command::new(&sg).arg("build").arg(source()).arg("-o").arg(&program));
    assert!(built.status.success(), "sg build failed:\n{}", text(&built));
    vec![("interpreted", vec![sg, source().into_os_string()]), ("compiled", vec![program.into_os_string()])]
}

/// The bytes of a name as SG gives them back: on Windows its units as WTF-8.
#[cfg(unix)]
fn name_of(bytes: &[u8]) -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(bytes.to_vec())
}

#[cfg(windows)]
fn name_of(units: &[u16]) -> OsString {
    use std::os::windows::ffi::OsStringExt;
    OsString::from_wide(units)
}

/// The names tried, and the bytes `Path.systemBytes` must give for each.
#[cfg(unix)]
fn names(in_a_folder: bool) -> Vec<(OsString, Vec<u8>)> {
    let mut made: Vec<Vec<u8>> = Vec::new();
    // Names that are UTF-8, the escape among them, which every system takes.
    made.push("plain çalışma".as_bytes().to_vec());
    made.push("x\u{10FF80}y\u{10FFFD}".as_bytes().to_vec());
    // What is not UTF-8: every byte from 0x80 alone, cut short, overlong, a half of a pair, past
    // U+10FFFF. A folder on macOS takes none of them.
    if !(in_a_folder && cfg!(target_os = "macos")) {
        for byte in 0x80..=0xFF_u8 {
            made.push(vec![b'n', byte]);
        }
        for bytes in [&[0xE6, 0x97][..], &[0xC0, 0x80], &[0xED, 0xA0, 0x80], &[0xF4, 0x90, 0x80, 0x80], &[0xF0, 0x9F, 0x41], &[b'a', 0xFF, 0xFE, b'b']] {
            made.push(bytes.to_vec());
        }
    }
    made.into_iter().map(|bytes| (name_of(&bytes), bytes)).collect()
}

#[cfg(windows)]
fn names(_in_a_folder: bool) -> Vec<(OsString, Vec<u8>)> {
    let made: Vec<Vec<u16>> = vec![
        "plain çalışma".encode_utf16().collect(),
        "x\u{10FF80}y".encode_utf16().collect(),
        // Halves of pairs alone: high, low, both in the wrong order, at the end.
        vec![u16::from(b'a'), 0xD800, u16::from(b'b')],
        vec![u16::from(b'a'), 0xDC00, u16::from(b'b')],
        vec![u16::from(b'a'), 0xDFFF, 0xD800, u16::from(b'b')],
        vec![u16::from(b'z'), 0xDBFF],
    ];
    made.into_iter()
        .map(|units| {
            let name = name_of(&units);
            let bytes = name.as_encoded_bytes().to_vec();
            (name, bytes)
        })
        .collect()
}

#[test]
fn words_that_are_not_text_come_as_they_are() {
    let dir = work("words");
    let names = names(false);
    for (how, words) in launchers(&dir) {
        let out = finished(Command::new(&words[0]).args(&words[1..]).arg("echo").args(names.iter().map(|(name, _)| name)));
        assert!(out.status.success(), "{how}: {}", text(&out));
        let said: Vec<String> = String::from_utf8(out.stdout).unwrap().lines().map(str::to_string).collect();
        let wanted: Vec<String> = names.iter().map(|(_, bytes)| hex(bytes)).collect();
        assert_eq!(said, wanted, "{how}");
    }
}

#[test]
fn names_in_a_folder_that_are_not_text() {
    let dir = work("folder");
    let launchers = launchers(&dir);
    for (how, words) in &launchers {
        let folder = dir.join(format!("files-{how}"));
        std::fs::create_dir_all(&folder).unwrap();
        let names = names(true);
        for (name, bytes) in &names {
            std::fs::write(folder.join(name), hex(bytes)).unwrap_or_else(|error| panic!("cannot make {}: {error}", hex(bytes)));
        }
        let out = finished(Command::new(&words[0]).args(&words[1..]).arg("walk").arg(&folder).args(words.iter()));
        assert!(out.status.success(), "{how}: {}", text(&out));
        let said = String::from_utf8(out.stdout).expect("the program writes hex");
        let mut lines: Vec<&str> = said.lines().collect();
        lines.sort();
        let mut wanted: Vec<String> = names.iter().map(|(_, bytes)| format!("{0} held true info true child {0} gone true", hex(bytes))).collect();
        wanted.sort();
        assert_eq!(lines, wanted, "{how}");
        let left: Vec<_> = std::fs::read_dir(&folder).unwrap().flatten().map(|entry| entry.file_name()).collect();
        assert!(left.is_empty(), "{how}: left {left:?}");
    }
}
