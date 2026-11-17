//! What the tests on Windows leave for a later slice: `tests/windows-pending.txt`, read by every
//! file of tests.

use std::path::Path;

/// Why a test or a program waits for a later slice on Windows: its line in
/// `tests/windows-pending.txt` (`name  # why`, a `*` at the end of the name for every name that
/// starts so). `None` on Linux and macOS, and for what runs on Windows already.
pub fn waits_on_windows(name: &str) -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    static LIST: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
    let list = LIST.get_or_init(|| {
        let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/windows-pending.txt")).unwrap();
        text.lines()
            .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
            .map(|line| {
                let (name, why) = line.split_once('#').unwrap_or((line, ""));
                (name.trim().to_string(), why.trim().to_string())
            })
            .collect()
    });
    list.iter()
        .find(|(listed, _)| match listed.strip_suffix('*') {
            Some(prefix) => name.starts_with(prefix),
            None => listed == name,
        })
        .map(|(_, why)| why.clone())
}

