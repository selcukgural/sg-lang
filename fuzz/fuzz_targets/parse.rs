//! The lexer and the parser, on any text: each ends with a tree or a message, never a panic,
//! a hang or a stack run out (choice 1350). Run on a stack as large as the one `sg` gives them.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // `sg` does not read a file that is not UTF-8.
    let Ok(text) = std::str::from_utf8(data) else { return };
    sg_fuzz::on_large_stack(|| {
        if let Err(diag) = sg::parser::parse(text) {
            let _ = diag.render(text, "fuzz.sg");
        }
    });
});
