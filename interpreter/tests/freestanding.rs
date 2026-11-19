//! Programs built without an operating system (`sg build --freestanding`). What the build
//! refuses (`tests/freestanding/error_*.sg`, each with the message in its `.out`); programs that
//! are built, held to every warning of the C compiler, compiled for machines without a system
//! where Clang knows them, and run here with a machine of this computer (`board.c`), their
//! output in their `.out` (`ok_*.sg`); `examples/bare` with its machine; and the `vsnprintf`
//! the runtime has there, held to the C library's.
//! To regenerate the expected outputs: `UPDATE_GOLDEN=1 cargo test`

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn files(prefix: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(root().join("tests/freestanding"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "sg") && path.file_name().unwrap().to_string_lossy().starts_with(prefix))
        .collect();
    found.sort();
    found
}

/// The Clang that compiles for a machine named by its target: the one `CC` names, as `sg build`
/// takes it, or the first `clang` of the path; on Windows the one `sg build` finds.
fn cross_clang() -> String {
    if cfg!(windows) {
        return host_cc();
    }
    std::env::var("CC").ok().filter(|cc| !cc.is_empty()).unwrap_or_else(|| "clang".to_string())
}

/// The C compiler of this machine, as `sg build` finds it: `cc` on Linux and macOS, Clang on
/// Windows.
fn host_cc() -> String {
    sg::cc::compiler().unwrap_or_else(|_| "cc".to_string())
}

fn has_cc() -> bool {
    Command::new(host_cc()).arg("--version").output().is_ok()
}

/// A program of this machine that a test builds: what the C compiler is given to link it (on
/// Windows LLVM's linker and C's library in the program, as `sg build` gives them), and its file.
fn host_link(program: &Path) -> (Command, PathBuf) {
    let program = PathBuf::from(sg::cc::program_file(&program.to_string_lossy()));
    let mut cc = Command::new(host_cc());
    cc.args(sg::cc::compile_flags(false)).arg("-o").arg(&program);
    (cc, program)
}

/// A tool of LLVM: on Windows the one beside Clang, where its installer puts them all, else the
/// one of the path; `nm` stands for `llvm-nm` there.
fn llvm_tool(name: &str) -> std::ffi::OsString {
    if !cfg!(windows) {
        return name.into();
    }
    let name = if name == "nm" { "llvm-nm" } else { name };
    let beside = sg::cc::compiler().ok().and_then(|cc| Path::new(&cc).parent().map(|dir| dir.join(format!("{name}.exe")))).filter(|path| path.exists());
    beside.map_or_else(|| name.into(), PathBuf::into_os_string)
}

/// The warnings every build here is held to, as in `golden.rs`.
const WARNINGS: [&str; 4] = ["-Wall", "-Wextra", "-Werror", "-Wno-unused-function"];

/// Compiles C of a program without an operating system into an object, for this machine or for
/// `target`.
fn compile(c_file: &Path, object: &Path, target: Option<&str>, release: bool) {
    let mut cc = Command::new(if target.is_some() { cross_clang() } else { host_cc() });
    cc.args(["-std=c11", "-ffreestanding", "-fno-stack-protector", "-ffp-contract=off", "-c", if release { "-O2" } else { "-O0" }]).args(WARNINGS);
    if let Some(target) = target {
        cc.arg(format!("--target={target}"));
    }
    let made = cc.arg("-o").arg(object).arg(c_file).output().unwrap();
    assert!(made.status.success(), "{} for {target:?}:\n{}", c_file.display(), String::from_utf8_lossy(&made.stderr));
}

/// The names an object takes from outside: what the program declares in `extern` blocks, and
/// the functions of the C compiler's own library (`__udivti3`, Windows' `__chkstk`), which start
/// with `__`, and `_fltused`, which Clang asks for on Windows where fractional numbers are used.
/// Nothing of the C library.
fn undefined_names_are_allowed(object: &Path, allowed: &[&str]) {
    let listed = Command::new(llvm_tool("nm")).arg("-u").arg(object).output().unwrap();
    assert!(listed.status.success(), "nm: {}", String::from_utf8_lossy(&listed.stderr));
    for line in String::from_utf8_lossy(&listed.stdout).lines() {
        let name = line.split_whitespace().last().unwrap_or_default();
        // An object of macOS has `_` in front of every name of C, and `nm -u` lists only the
        // names there; for an ELF object it writes `U` in front of each.
        let mach_o = cfg!(target_os = "macos") && !line.trim_start().starts_with("U ");
        let name = if mach_o { name.strip_prefix('_').unwrap_or(name) } else { name };
        assert!(name.starts_with("__") || name == "_fltused" || allowed.contains(&name), "{} takes `{name}` from outside", object.display());
    }
    // Nor does an object of Windows ask the linker for C's library (`/DEFAULTLIB`), as Clang has
    // it ask when it is told which C library a program has.
    if cfg!(windows) {
        let read = Command::new(llvm_tool("llvm-readobj")).arg("--coff-directives").arg(object).output().unwrap();
        let said = String::from_utf8_lossy(&read.stdout);
        assert!(!said.contains("DEFAULTLIB"), "{} asks for a library of C: {said}", object.display());
    }
}

/// The machines without an operating system this Clang (`CC`, or `clang`) compiles for, of
/// those tried.
fn targets() -> Vec<&'static str> {
    let probe = std::env::temp_dir().join(format!("sg-probe-{}.c", std::process::id()));
    let object = probe.with_extension("o");
    std::fs::write(&probe, "int f(void) { return 1; }\n").unwrap();
    let found = ["aarch64-none-elf", "x86_64-none-elf", "riscv64-unknown-elf"]
        .into_iter()
        .filter(|target| {
            Command::new(cross_clang()).args(["-ffreestanding", "-c", "-o"]).arg(&object).arg(format!("--target={target}")).arg(&probe).output().is_ok_and(|made| made.status.success())
        })
        .collect();
    let _ = std::fs::remove_file(&probe);
    let _ = std::fs::remove_file(&object);
    found
}

#[test]
fn refused() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut failures = Vec::new();
    let found = files("error_");
    assert!(found.len() > 10);
    for path in found {
        let src = std::fs::read_to_string(&path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let actual = match sg::emit_c_freestanding(&sg::Sources::single(&src, &name), true) {
            Ok(_) => "built\n".to_string(),
            Err(diag) => diag.render(&src, &name),
        };
        let golden = path.with_extension("out");
        if update {
            std::fs::write(&golden, &actual).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&golden).unwrap_or_default();
        if actual != expected {
            failures.push(format!("--- {name}\nexpected:\n{expected}\nactual:\n{actual}"));
        }
    }
    assert!(failures.is_empty(), "{} file(s) differ:\n{}", failures.len(), failures.join("\n"));
}

/// Each program is built, debug and release, compiled for each machine Clang knows here, and
/// run with `board.c`; its output, the panic included, is its `.out`, and an exit code other
/// than 0 is the last line.
#[test]
fn built_and_run() {
    if !has_cc() {
        eprintln!("no C compiler (`cc`) found: programs without an operating system are not built");
        return;
    }
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let work = root().join("target").join("freestanding");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let targets = targets();
    let board = root().join("tests/freestanding/board.c");
    let mut failures = Vec::new();
    for path in files("ok_") {
        let src = std::fs::read_to_string(&path).unwrap();
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let name = format!("{stem}.sg");
        let mut seen: Option<String> = None;
        for release in [false, true] {
            let code = sg::emit_c_freestanding(&sg::Sources::single(&src, &name), !release).unwrap_or_else(|diag| panic!("{}", diag.render(&src, &name)));
            let c_file = work.join(format!("{stem}-{release}.c"));
            std::fs::write(&c_file, code).unwrap();
            for target in &targets {
                let object = work.join(format!("{stem}-{release}-{target}.o"));
                compile(&c_file, &object, Some(target), release);
                undefined_names_are_allowed(&object, &["board_write", "board_stop", "board_memory", "board_memory_size"]);
            }
            let object = work.join(format!("{stem}-{release}.o"));
            compile(&c_file, &object, None, release);
            undefined_names_are_allowed(&object, &["board_write", "board_stop", "board_memory", "board_memory_size"]);
            let (mut cc, program) = host_link(&work.join(format!("{stem}-{release}")));
            let linked = cc.arg(&board).arg(&object).output().unwrap();
            assert!(linked.status.success(), "{stem}: {}", String::from_utf8_lossy(&linked.stderr));
            let ran = Command::new(&program).output().unwrap();
            let mut text = String::from_utf8_lossy(&ran.stdout).replace("\r\n", "\n");
            match ran.status.code() {
                Some(0) => {}
                Some(code) => text.push_str(&format!("exit code {code}\n")),
                None => text.push_str("ended by a signal\n"),
            }
            // A release build wraps where a debug build panics: only the debug one is compared.
            if release {
                continue;
            }
            seen = Some(text);
        }
        let actual = seen.expect("the debug build ran");
        let golden = path.with_extension("out");
        if update {
            std::fs::write(&golden, &actual).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&golden).unwrap_or_default();
        if actual != expected {
            failures.push(format!("--- {name}\nexpected:\n{expected}\nactual:\n{actual}"));
        }
    }
    assert!(failures.is_empty(), "{} program(s) differ:\n{}", failures.len(), failures.join("\n"));
}

/// When `onPanic` returns, the processor waits for ever: nothing after the panic runs, and a
/// panic inside `onPanic` is not told.
#[test]
fn waits_after_a_panic() {
    if !has_cc() {
        return;
    }
    let work = root().join("target").join("freestanding-waits");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let path = root().join("tests/freestanding/waits_after_panic.sg");
    let src = std::fs::read_to_string(&path).unwrap();
    let code = sg::emit_c_freestanding(&sg::Sources::single(&src, "waits_after_panic.sg"), true).unwrap_or_else(|diag| panic!("{}", diag.render(&src, "waits_after_panic.sg")));
    let c_file = work.join("waits.c");
    std::fs::write(&c_file, code).unwrap();
    let object = work.join("waits.o");
    compile(&c_file, &object, None, false);
    let (mut cc, program) = host_link(&work.join("waits"));
    let linked = cc.arg(root().join("tests/freestanding/board.c")).arg(&object).output().unwrap();
    assert!(linked.status.success(), "{}", String::from_utf8_lossy(&linked.stderr));
    let out = work.join("waits.out");
    let mut child = Command::new(&program).stdout(std::fs::File::create(&out).unwrap()).spawn().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let still = child.try_wait().unwrap().is_none();
    let _ = child.kill();
    let _ = child.wait();
    assert!(still, "the program ended after its `onPanic` returned");
    let written = std::fs::read_to_string(&out).unwrap().replace("\r\n", "\n");
    assert_eq!(
        written,
        "before\nafter panic: index 3 is out of range: the length is 3\n  --> waits_after_panic.sg:13:12\n 13 |     return items[3]\n    |            ^^^^^^^^\n  hint: `.get(i)` reads with `null` instead; before a write, compare the index with `.length()`\nonPanic ends\n"
    );
}

/// `examples/bare`, built by `sg build` debug and release, with the machine of its tests: what it
/// writes, the panic its last call ends in, and the code its `onPanic` stops the machine with.
#[test]
fn bare() {
    if !has_cc() {
        return;
    }
    let sg_bin = env!("CARGO_BIN_EXE_sg");
    let example = root().join("examples/bare");
    let work = root().join("target").join("freestanding-bare");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let expected = std::fs::read_to_string(example.join("machine.out")).unwrap();
    for release in [false, true] {
        let object = work.join(format!("bare-{release}.o"));
        let mut build = Command::new(sg_bin);
        build.args(["build", "bare", "--freestanding", "-o"]).arg(&object).current_dir(root().join("examples"));
        if release {
            build.arg("--release");
        }
        let made = build.output().unwrap();
        let said = String::from_utf8_lossy(&made.stdout);
        assert!(made.status.success() && said.starts_with("built ") && said.contains("without an operating system"), "{said}{}", String::from_utf8_lossy(&made.stderr));
        undefined_names_are_allowed(&object, &["board_write", "board_stop", "board_memory", "board_memory_size"]);
        let (mut cc, program) = host_link(&work.join(format!("machine-{release}")));
        cc.arg("-std=c11").args(["-Wall", "-Wextra", "-Werror", "-pedantic"]).arg(example.join("machine.c")).arg(&object);
        let linked = cc.output().unwrap();
        assert!(linked.status.success(), "{}", String::from_utf8_lossy(&linked.stderr));
        let ran = Command::new(&program).output().unwrap();
        // On Windows the machine's `printf` ends a line with `\r\n`, and the place of the panic
        // is the path as `sg build` was given it, with `\`.
        let said = String::from_utf8_lossy(&ran.stdout).replace("\r\n", "\n").replace("bare\\main.sg", "bare/main.sg");
        assert_eq!(said, expected, "release: {release}");
        assert_eq!(ran.status.code(), Some(101), "release: {release}");
    }
    for target in targets() {
        let object = work.join(format!("bare-{target}.o"));
        let made = Command::new(sg_bin).args(["build", "bare", "--freestanding", "--target", target, "--release", "-o"]).arg(&object).current_dir(root().join("examples")).output().unwrap();
        assert!(made.status.success(), "{target}: {}", String::from_utf8_lossy(&made.stderr));
        undefined_names_are_allowed(&object, &["board_write", "board_stop", "board_memory", "board_memory_size"]);
    }
    // A target is for a program without an operating system.
    let stray = Command::new(sg_bin).args(["build", "bare", "--target", "aarch64-none-elf"]).current_dir(root().join("examples")).output().unwrap();
    assert_eq!(stray.status.code(), Some(2));
    // Such a program has no `main` to run.
    let run = Command::new(sg_bin).arg("bare").current_dir(root().join("examples")).output().unwrap();
    assert!(String::from_utf8_lossy(&run.stderr).contains("is built with `sg build <folder> --freestanding`"), "{}", String::from_utf8_lossy(&run.stderr));
}

/// The `vsnprintf` a program without an operating system has writes what the C library's
/// writes, for the formats the runtime and the programs use.
#[test]
fn formats_as_printf_does() {
    if !has_cc() {
        return;
    }
    let work = root().join("target").join("freestanding-formats");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let ours = format!(
        "#define SG_HEAP 0\n{}\nstatic _Noreturn void sg_fs_halt(void) {{ for (;;) {{ }} }}\nstatic _Noreturn void sg_fs_no_fixed(void) {{ for (;;) {{ }} }}\nstatic _Noreturn void sg_fs_no_parse(void) {{ for (;;) {{ }} }}\n\
         int fs_format(char* out, size_t room, const char* format, ...) {{\n    va_list args;\n    va_start(args, format);\n    int n = sg_fs_vsnprintf(out, room, format, args);\n    va_end(args);\n    return n;\n}}\n",
        sg::emit_c::freestanding_prelude()
    );
    std::fs::write(work.join("ours.c"), ours).unwrap();
    let cases = r#"#include <stdio.h>
#include <string.h>
#include <stdint.h>
int fs_format(char* out, size_t room, const char* format, ...);
static int failed = 0;
#define SAME(room, ...) do { \
    char a[64], b[64]; memset(a, 'x', sizeof a); memset(b, 'x', sizeof b); \
    int na = snprintf(a, room, __VA_ARGS__), nb = fs_format(b, room, __VA_ARGS__); \
    if (na != nb || memcmp(a, b, room) != 0) { failed++; printf("differs: %s: \"%.*s\" %d, ours \"%.*s\" %d\n", #__VA_ARGS__, (int)room, a, na, (int)room, b, nb); } \
} while (0)
int main(void) {
    long long values[] = { 0, 1, -1, 42, -17, 9223372036854775807LL, -9223372036854775807LL - 1, 255, 1000000 };
    for (unsigned i = 0; i < sizeof values / sizeof values[0]; i++) {
        long long v = values[i];
        SAME(64, "%lld|%8lld|%-8lld|%08lld|%.3lld|%8.4lld", v, v, v, v, v, v);
        SAME(64, "%llu|%llx|%llX|%.4llX|%.0llx|%12.6llx", (unsigned long long)v, (unsigned long long)v, (unsigned long long)v, (unsigned long long)v, (unsigned long long)v, (unsigned long long)v);
        SAME(64, "%d|%u|%x|%5d|%-5u", (int)v, (unsigned)v, (unsigned)v, (int)v, (unsigned)v);
        SAME(64, "%zu|%ld", (size_t)v, (long)v);
        SAME(8, "%lld and more", v);
    }
    SAME(64, "%s|%.3s|%10.*s|%-*.*s|%*.*s|", "text", "texts", 2, "abc", 6, 3, "abcdef", 7, 2, "xy");
    SAME(64, "%c%c|%%|100%%", 'o', 'k');
    SAME(64, "%.*s and %.*s", 0, "", 4, "four");
    SAME(1, "%s", "nothing fits");
    SAME(5, "%s", "just over");
    if (failed) return 1;
    printf("same\n");
    return 0;
}
"#;
    std::fs::write(work.join("cases.c"), cases).unwrap();
    let (mut cc, program) = host_link(&work.join("formats"));
    let made = cc.args(["-std=c11", "-Wno-format-truncation"]).arg(work.join("cases.c")).arg(work.join("ours.c")).output().unwrap();
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success() && String::from_utf8_lossy(&ran.stdout).trim_end() == "same", "{}", String::from_utf8_lossy(&ran.stdout));
}

/// The division of 128 bits a program without an operating system has, in case the C compiler's
/// library is missing, divides as that library does: for numbers of every size and sign, the
/// smallest of them included.
#[test]
fn divides_as_the_compiler_does() {
    if !has_cc() {
        return;
    }
    let work = root().join("target").join("freestanding-division");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    // Under other names, so that the program's own `/` and `%` still go to the C compiler's.
    let ours = format!(
        "#include <stdint.h>\n#define SG_FS_PLAIN\n#define __udivti3 fs_udivti3\n#define __umodti3 fs_umodti3\n#define __divti3 fs_divti3\n#define __modti3 fs_modti3\n{}",
        sg::emit_c::freestanding_division()
    );
    std::fs::write(work.join("ours.c"), ours).unwrap();
    let cases = r#"#include <stdio.h>
#include <stdint.h>
typedef unsigned __int128 u128;
typedef __int128 i128;
u128 fs_udivti3(u128 a, u128 b);
u128 fs_umodti3(u128 a, u128 b);
i128 fs_divti3(i128 a, i128 b);
i128 fs_modti3(i128 a, i128 b);
static uint64_t state = 0x9e3779b97f4a7c15u;
static uint64_t next(void) {
    state ^= state << 13; state ^= state >> 7; state ^= state << 17;
    return state;
}
/* A number of a random size: from a few bits to all 128. */
static u128 sized(void) {
    u128 n = (u128)next() << 64 | next();
    return n >> (next() % 128);
}
int main(void) {
    int failed = 0;
    i128 smallest = (i128)((u128)1 << 127);
    i128 edges[] = { 0, 1, -1, 2, -2, 7, -7, (i128)INT64_MAX, (i128)INT64_MIN, smallest, smallest + 1, -(smallest + 1) };
    int count = (int)(sizeof edges / sizeof edges[0]);
    for (int k = 0; k < 200000 + count * count; k++) {
        u128 ua, ub;
        if (k < count * count) { ua = (u128)edges[k / count]; ub = (u128)edges[k % count]; }
        else { ua = sized(); ub = sized(); }
        if (ub == 0) continue;
        if (fs_udivti3(ua, ub) != ua / ub || fs_umodti3(ua, ub) != ua % ub) failed++;
        i128 a = (i128)ua, b = (i128)ub;
        /* The one quotient that does not fit, which the runtime never asks for. */
        if (a == smallest && b == -1) continue;
        if (fs_divti3(a, b) != a / b || fs_modti3(a, b) != a % b) failed++;
    }
    if (failed) { printf("%d differ\n", failed); return 1; }
    printf("same\n");
    return 0;
}
"#;
    std::fs::write(work.join("cases.c"), cases).unwrap();
    let (mut cc, program) = host_link(&work.join("division"));
    cc.args(["-std=c11", "-O2"]).args(WARNINGS).arg(work.join("cases.c")).arg(work.join("ours.c"));
    // Windows does not link the C compiler's library on its own: it is named, as Clang has it.
    if cfg!(windows) {
        let asked = Command::new(host_cc()).args(["--rtlib=compiler-rt", "-print-libgcc-file-name"]).output().unwrap();
        let library = String::from_utf8_lossy(&asked.stdout).trim().to_string();
        assert!(Path::new(&library).exists(), "Clang's library of its own (`{library}`) is not there: install LLVM whole");
        cc.arg(library);
    }
    let made = cc.output().unwrap();
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success() && String::from_utf8_lossy(&ran.stdout).trim_end() == "same", "{}", String::from_utf8_lossy(&ran.stdout));
}

/// A `const assert` that measures a type is left to the compiler of C when the program is built
/// for another machine (`--target`), whose layout the checker does not know; on this machine
/// the checker refuses it, and compiled code hands it to `_Static_assert` either way.
#[test]
fn measured_for_the_target() {
    let src = "const assert(Memory.sizeOf<nint>() == 4, \"a word of four bytes\")\n\nexport int start() {\n    return 1\n}\n\nvoid onPanic(read string message) {\n}\n";
    let sources = || sg::Sources::single(src, "measured.sg");
    let here = sg::emit_c_freestanding(&sources(), true).expect_err("refused on this machine");
    assert!(here.msg.contains("const assert failed: a word of four bytes"), "{}", here.msg);
    let code = sg::emit_c_freestanding_for(&sources(), true, true).unwrap_or_else(|diag| panic!("{}", diag.render(src, "measured.sg")));
    assert!(code.contains("_Static_assert((((int64_t)sizeof(int64_t)) == 4LL), \"measured.sg:1: const assert failed: a word of four bytes\");"), "no `_Static_assert` in the C");
}
