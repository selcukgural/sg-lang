#!/bin/sh
# Builds the kernel, links it and boots it in QEMU: `examples/kernel/run.sh` from the folder of
# the interpreter, or `examples/kernel/run.sh --release`. It needs a Clang that knows RISC-V
# (`CC`, or `clang`; Apple's knows only its own: `CC=/opt/homebrew/opt/llvm/bin/clang`), LLD
# (`ld.lld`, found on the path or beside that Clang) and `qemu-system-riscv64`.
set -e
cd "$(dirname "$0")/../.."
CC="${CC:-clang}"
export CC
work="target/kernel"
mkdir -p "$work"
cargo run --release --quiet -- build examples/kernel --freestanding "$@" --target riscv64-unknown-elf -o "$work/kernel.o" -- -mcmodel=medany -march=rv64gc -mabi=lp64d
"$CC" --target=riscv64-unknown-elf -march=rv64gc -mabi=lp64d -nostdlib -fuse-ld=lld -Wl,--gc-sections -T examples/kernel/kernel.ld "$work/kernel.o" -o "$work/kernel.elf"
exec qemu-system-riscv64 -machine virt -bios none -m 128M -nographic -kernel "$work/kernel.elf"
