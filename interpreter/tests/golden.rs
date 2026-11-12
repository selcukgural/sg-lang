//! Runs every `.sg` file and compares the output with the `.out` file next to it.
//! `.out`: what the program printed, followed by the error or panic message if any.
//! To regenerate the expected outputs: `UPDATE_GOLDEN=1 cargo test`

use std::path::{Path, PathBuf};

fn run_file(path: &Path) -> String {
    let src = std::fs::read_to_string(path).unwrap();
    let name = path.file_name().unwrap().to_string_lossy();
    let mut out = Vec::new();
    // A file named `tests_...` is run by the test command.
    let result = match name.starts_with("tests_") {
        true => sg::run_tests(&src, &name, &mut out).map(|_| ()),
        false => sg::run(&src, &mut out),
    };
    let mut text = String::from_utf8(out).unwrap();
    match result {
        Ok(()) => {}
        Err(sg::Stop::Diag(diag)) => text.push_str(&diag.render(&src, &name)),
        Err(sg::Stop::Failed(error)) => text.push_str(&format!("error: {error}\n")),
        Err(other) => text.push_str(&format!("{other:?}\n")),
    }
    text
}

#[test]
fn golden() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut files: Vec<PathBuf> = Vec::new();
    for dir in ["examples", "tests/cases"] {
        for entry in std::fs::read_dir(root.join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "sg") {
                files.push(path);
            }
        }
    }
    files.sort();
    assert!(!files.is_empty());

    let mut failures = Vec::new();
    for path in &files {
        let actual = run_file(path);
        let golden = path.with_extension("out");
        if update {
            std::fs::write(&golden, &actual).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&golden).unwrap_or_default();
        if actual != expected {
            failures.push(format!(
                "--- {}\nexpected:\n{expected}\nactual:\n{actual}",
                path.strip_prefix(root).unwrap().display()
            ));
        }
    }
    assert!(failures.is_empty(), "{} file(s) differ:\n{}", failures.len(), failures.join("\n"));
}
