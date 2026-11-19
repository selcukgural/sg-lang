//! All of `sg build` before the C compiler runs, on any text: the parser, the checker, the
//! consts it finds, and the C written from the program (choice 1350). Each ends with C or a
//! message, never a panic, a hang or a stack run out. `sg::emit_c` runs on the stack `sg` gives.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // `sg` does not read a file that is not UTF-8.
    let Ok(text) = std::str::from_utf8(data) else { return };
    // A debug build (overflow panics) for an even length, a release build (it wraps) for an odd.
    let checked = data.len() % 2 == 0;
    let sources = sg::Sources::single(text, sg_fuzz::PATH);
    if let Err(diag) = sg::emit_c_sources(&sources, checked) {
        let _ = diag.render_in(&sources);
    }
});
