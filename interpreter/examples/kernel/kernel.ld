/* Where the kernel lies in the memory of QEMU's `virt` machine, which starts its first core in
   machine mode at 0x8000_0000 when it is given no firmware (`-bios none`). The start code
   (`_start` in machine.c, in `.text.start`) comes first; after the kernel come its stack and the
   memory of its heap, up to the last page of the 128 MB. The tests leave word in that page of
   what they want (a fault, to see a panic told): QEMU's `loader` device writes it before the
   machine starts. */
OUTPUT_ARCH(riscv)
ENTRY(_start)

MEMORY {
    RAM (rwx) : ORIGIN = 0x80000000, LENGTH = 128M
}

SECTIONS {
    .text : {
        *(.text.start)
        *(.text .text.*)
    } > RAM
    .rodata : ALIGN(16) {
        *(.rodata .rodata.* .srodata .srodata.*)
    } > RAM
    .data : ALIGN(16) {
        *(.data .data.* .sdata .sdata.*)
    } > RAM
    .bss (NOLOAD) : ALIGN(16) {
        __bss_start = .;
        *(.sbss .sbss.* .bss .bss.* COMMON)
        . = ALIGN(16);
        __bss_end = .;
    } > RAM
    .stack (NOLOAD) : ALIGN(16) {
        . += 64K;
        __stack_top = .;
    } > RAM
    __heap_start = .;
    __heap_end = ORIGIN(RAM) + LENGTH(RAM) - 4K;
    __asked = __heap_end;
}
