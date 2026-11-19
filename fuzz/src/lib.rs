//! What the fuzz targets share.

/// The path a fuzzed program is said to have: a folder that is not there, so that what it
/// takes in from beside it (`embed`, `extern "./x.c"`) is never found, and each input gives the
/// same on every run.
pub const PATH: &str = "/sg-fuzz-nowhere/fuzz.sg";

/// Runs `work` on a stack as large as the one `sg` runs the front end on (`sg::STACK_BYTES` is
/// not public; this is its size).
pub fn on_large_stack(work: impl FnOnce() + Send) {
    std::thread::scope(|scope| {
        std::thread::Builder::new().stack_size(1 << 30).spawn_scoped(scope, work).expect("could not start a thread").join().expect("the work panicked");
    });
}
