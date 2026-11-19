//! `Regex` of the language's own module, on any pattern and text (choice 1468): the input up to its
//! first line break is the pattern, the rest the text. The pattern is compiled when the program
//! runs, and a regular expression is then matched every way there is. Each ends with a result
//! or a `RegexError`, never a panic of SG or of Rust, a hang or a stack run out.
#![no_main]

use libfuzzer_sys::fuzz_target;
use std::fmt::Write as _;

/// The text as an SG literal, each character written by its code.
fn literal(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        let _ = write!(out, "\\u{{{:x}}}", c as u32);
    }
    out.push('"');
    out
}

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else { return };
    let (pattern, text) = input.split_once('\n').unwrap_or((input, ""));
    let program = format!(
        "void main() {{
    var pattern = {}
    var text = {}
    match Regex.compile(read pattern) {{
        Ok(re) => {{
            _ = re.isMatch(read text)
            _ = re.find(read text)
            _ = re.findAll(read text)
            _ = re.captures(read text)
            _ = re.replaceAll(read text, \"[$0]\")
            _ = re.split(read text)
        }}
        Error(e) => _ = $\"{{e}}\".toString()
    }}
}}
",
        literal(pattern),
        literal(text)
    );
    let mut out = std::io::sink();
    if let Err(stop) = sg::run(&program, &mut out) {
        match stop {
            sg::Stop::Diag(diag) => panic!("{}", diag.render(&program, sg_fuzz::PATH)),
            _ => panic!("the program stopped"),
        }
    }
});
