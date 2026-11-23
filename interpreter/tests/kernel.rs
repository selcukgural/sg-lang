//! The kernel of `examples/kernel`, built for RISC-V by `sg build`, linked with its linker script
//! and booted in QEMU's `virt` machine, debug and release: what it writes to the serial port is
//! `examples/kernel/kernel.out`, and it powers the machine off with 0. Asked for a fault (QEMU's
//! `loader` device leaves the word in the last page of memory), it tells the panic on the serial
//! port and ends QEMU with 101.
//!
//! It needs a Clang that knows RISC-V (`CC`, or `clang`), LLD and `qemu-system-riscv64`; when one
//! is missing the test fails and says what to install, so that a check that did not run is never
//! taken for one that passed. To regenerate the expected output: `UPDATE_GOLDEN=1 cargo test`

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The flags of the machine: QEMU's `virt` gives its cores `rv64gc`, and the kernel lies at
/// 0x8000_0000, which only the `medany` model of code reaches.
const MACHINE: [&str; 3] = ["-mcmodel=medany", "-march=rv64gc", "-mabi=lp64d"];

const HOW: &str = "the kernel's test needs a Clang that knows RISC-V (`CC`, or `clang` on the path; Apple's knows only its own), LLD and QEMU: \
on macOS `brew install llvm lld qemu`, then `CC=$(brew --prefix llvm)/bin/clang PATH=$(brew --prefix lld)/bin:$PATH cargo test`; \
on Debian or Ubuntu `apt install clang lld qemu-system-misc` (`qemu-system-riscv` from Ubuntu 26.04) (the image of `tests/Containerfile` has them); \
on Windows LLVM's Clang has them all, and QEMU comes with `winget install SoftwareFreedomConservancy.QEMU`";

/// The Clang of `CC`, or `clang`; on Windows the one `sg build` finds.
fn clang() -> String {
    if cfg!(windows) {
        return sg::cc::compiler().unwrap_or_else(|_| "clang".to_string());
    }
    std::env::var("CC").ok().filter(|cc| !cc.is_empty()).unwrap_or_else(|| "clang".to_string())
}

/// QEMU's machine of RISC-V: the one of the path, or on Windows the one its installer puts in its
/// folder, which it does not add to the path.
fn qemu() -> std::ffi::OsString {
    const NAME: &str = "qemu-system-riscv64";
    let installed = Path::new(r"C:\Program Files\qemu").join(format!("{NAME}.exe"));
    let on_path = Command::new(NAME).arg("--version").output().is_ok_and(|ran| ran.status.success());
    match cfg!(windows) && !on_path && installed.exists() {
        true => installed.into_os_string(),
        false => NAME.into(),
    }
}

/// Runs QEMU on `elf`, with `extra` arguments, for at most 60 seconds: what the serial port
/// showed and the code QEMU ended with.
fn boot(elf: &Path, extra: &[&str], out: &Path) -> (String, Option<i32>) {
    let file = std::fs::File::create(out).unwrap();
    let mut qemu = Command::new(qemu())
        .args(["-machine", "virt", "-bios", "none", "-m", "128M", "-nographic", "-kernel"])
        .arg(elf)
        .args(extra)
        .stdin(Stdio::null())
        .stdout(file)
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("cannot run `qemu-system-riscv64`: {error}\n{HOW}"));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = qemu.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(60) {
            let _ = qemu.kill();
            let _ = qemu.wait();
            panic!("{} did not power the machine off in 60 seconds; it wrote:\n{}", elf.display(), std::fs::read_to_string(out).unwrap_or_default());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // QEMU's serial port writes a line end as `\r\n`.
    (std::fs::read_to_string(out).unwrap().replace("\r\n", "\n"), status.code())
}

#[test]
fn boots_in_qemu() {
    let cc = clang();
    let work = root().join("target").join("kernel-test");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    // Each tool first, so that a missing one says what to install.
    let probe = work.join("probe.c");
    std::fs::write(&probe, "int f(void) { return 1; }\n").unwrap();
    let knows = Command::new(&cc).args(["--target=riscv64-unknown-elf", "-c", "-o"]).arg(work.join("probe.o")).arg(&probe).output();
    // What went wrong is told: the compiler's own words, or why it could not be started.
    let why = match &knows {
        Ok(made) => String::from_utf8_lossy(&made.stderr).into_owned(),
        Err(error) => format!("it could not be started: {error}"),
    };
    assert!(knows.is_ok_and(|made| made.status.success()), "`{cc}` does not compile for RISC-V: {why}\n{HOW}");
    let linked = Command::new(&cc).args(["--target=riscv64-unknown-elf", "-nostdlib", "-fuse-ld=lld", "-Wl,-e,f", "-o"]).arg(work.join("probe.elf")).arg(work.join("probe.o")).output().unwrap();
    assert!(linked.status.success(), "`{cc}` cannot link with LLD: {}\n{HOW}", String::from_utf8_lossy(&linked.stderr));
    assert!(Command::new(qemu()).arg("--version").output().is_ok_and(|ran| ran.status.success()), "no `qemu-system-riscv64`\n{HOW}");

    let sg_bin = env!("CARGO_BIN_EXE_sg");
    let example = root().join("examples/kernel");
    let golden = example.join("kernel.out");
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut seen: Option<String> = None;
    for release in [false, true] {
        let object = work.join(format!("kernel-{release}.o"));
        let mut build = Command::new(sg_bin);
        build.args(["build", "examples/kernel", "--freestanding", "--target", "riscv64-unknown-elf", "-o"]).arg(&object).current_dir(root()).env("CC", &cc);
        if release {
            build.arg("--release");
        }
        build.arg("--").args(MACHINE).args(["-Wall", "-Wextra", "-Werror", "-Wno-unused-function"]);
        let made = build.output().unwrap();
        assert!(made.status.success(), "release: {release}: {}{}", String::from_utf8_lossy(&made.stdout), String::from_utf8_lossy(&made.stderr));
        // Linked with nothing but its own object: what the runtime needs of the C compiler's
        // library, it has.
        let elf: PathBuf = work.join(format!("kernel-{release}.elf"));
        let linked = Command::new(&cc)
            .arg("--target=riscv64-unknown-elf")
            .args(&MACHINE[1..])
            .args(["-nostdlib", "-fuse-ld=lld", "-Wl,--gc-sections", "-T"])
            .arg(example.join("kernel.ld"))
            .arg(&object)
            .arg("-o")
            .arg(&elf)
            .output()
            .unwrap();
        assert!(linked.status.success(), "release: {release}: {}", String::from_utf8_lossy(&linked.stderr));

        let (said, code) = boot(&elf, &[], &work.join(format!("serial-{release}.txt")));
        assert_eq!(code, Some(0), "release: {release}: it wrote:\n{said}");
        if update && !release {
            std::fs::write(&golden, &said).unwrap();
        } else {
            assert_eq!(said, std::fs::read_to_string(&golden).unwrap(), "release: {release}");
        }
        if let Some(before) = &seen {
            assert_eq!(&said, before, "debug and release differ");
        }
        seen = Some(said);

        // A fault, asked for: the word 1 in the last page of the 128 MB.
        let (said, code) = boot(&elf, &["-device", "loader,addr=0x87fff000,data=1,data-len=4"], &work.join(format!("fault-{release}.txt")));
        assert_eq!(code, Some(101), "release: {release}: it wrote:\n{said}");
        let lines: Vec<&str> = said.lines().collect();
        assert!(lines.len() >= 5, "release: {release}: {said}");
        assert_eq!(lines[2], "reading where nothing is", "release: {release}: {said}");
        assert!(lines[3].starts_with("panic: unexpected trap: a fault of a load at 0x8000"), "release: {release}: {said}");
        // On Windows the file is joined to the folder `sg build` was given with `\`.
        assert!(lines[4].trim_start().replace('\\', "/").starts_with("--> examples/kernel/main.sg:"), "release: {release}: {said}");
    }
}
