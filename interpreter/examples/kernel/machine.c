/* The part of the kernel SG has no words for: the instructions of RISC-V. The start of the first
   core, the vector every trap goes through, and the registers of the core itself (its CSRs) and
   `wfi`. It is compiled into the same unit as the kernel (`extern "./machine.c"` in main.sg), so
   that its `static inline` functions cost a call no more than an instruction. The devices (the
   serial port, the timer, the device that powers the machine off) are driven in SG, through
   `Memory.readVolatile` and `writeVolatile`. */
#include <stddef.h>
#include <stdint.h>

extern char __heap_start[], __heap_end[], __asked[];

/* The fractional registers, kept by the vector when the kernel is compiled for a core that has
   them (`-march=rv64gc`), with their state (`fcsr`). */
#if defined(__riscv_flen) && __riscv_flen >= 64
#define MACHINE_FP_ON \
        "li t0, 0x2000\n" \
        "csrs mstatus, t0\n" \
        "csrw fcsr, zero\n"
#define MACHINE_FP_SAVE \
    "fsd ft0, 128(sp)\n"  "fsd ft1, 136(sp)\n"  "fsd ft2, 144(sp)\n"  "fsd ft3, 152(sp)\n" \
    "fsd ft4, 160(sp)\n"  "fsd ft5, 168(sp)\n"  "fsd ft6, 176(sp)\n"  "fsd ft7, 184(sp)\n" \
    "fsd ft8, 192(sp)\n"  "fsd ft9, 200(sp)\n"  "fsd ft10, 208(sp)\n" "fsd ft11, 216(sp)\n" \
    "fsd fa0, 224(sp)\n"  "fsd fa1, 232(sp)\n"  "fsd fa2, 240(sp)\n"  "fsd fa3, 248(sp)\n" \
    "fsd fa4, 256(sp)\n"  "fsd fa5, 264(sp)\n"  "fsd fa6, 272(sp)\n"  "fsd fa7, 280(sp)\n" \
    "frcsr t0\n" \
    "sd t0, 288(sp)\n"
#define MACHINE_FP_LOAD \
    "ld t0, 288(sp)\n" \
    "fscsr t0\n" \
    "fld ft0, 128(sp)\n"  "fld ft1, 136(sp)\n"  "fld ft2, 144(sp)\n"  "fld ft3, 152(sp)\n" \
    "fld ft4, 160(sp)\n"  "fld ft5, 168(sp)\n"  "fld ft6, 176(sp)\n"  "fld ft7, 184(sp)\n" \
    "fld ft8, 192(sp)\n"  "fld ft9, 200(sp)\n"  "fld ft10, 208(sp)\n" "fld ft11, 216(sp)\n" \
    "fld fa0, 224(sp)\n"  "fld fa1, 232(sp)\n"  "fld fa2, 240(sp)\n"  "fld fa3, 248(sp)\n" \
    "fld fa4, 256(sp)\n"  "fld fa5, 264(sp)\n"  "fld fa6, 272(sp)\n"  "fld fa7, 280(sp)\n"
#else
#define MACHINE_FP_ON ""
#define MACHINE_FP_SAVE ""
#define MACHINE_FP_LOAD ""
#endif

/* Where QEMU starts every core, in machine mode, at 0x8000_0000 (`.text.start` is laid first).
   The first core (`mhartid` 0) takes the stack the linker script leaves, clears the memory of
   the variables of C that start at zero (the runtime's few), sends every trap to the vector
   below, turns the unit of fractional numbers
   on where the core has one (it starts off in machine mode, and an instruction of it would trap)
   and calls `kernelMain`; the other cores, and the first if `kernelMain` ever returns, wait for
   ever. */
__attribute__((naked, section(".text.start"))) void _start(void) {
    __asm__ volatile(
        "csrr t0, mhartid\n"
        "bnez t0, 3f\n"
        "la sp, __stack_top\n"
        "la t0, __bss_start\n"
        "la t1, __bss_end\n"
        "1: bgeu t0, t1, 2f\n"
        "sd zero, 0(t0)\n"
        "addi t0, t0, 8\n"
        "j 1b\n"
        "2: la t0, machine_trap_vector\n"
        "csrw mtvec, t0\n"
        "csrw mscratch, zero\n"
        MACHINE_FP_ON
        "call kernelMain\n"
        "3: wfi\n"
        "j 3b\n");
}

/* Every trap, an interrupt or an exception, comes here (`mtvec`), on the stack of what it
   stopped. The registers a call of C may change are kept on that stack, the integer ones and
   the fractional ones with their state; those a call keeps (`s0`-`s11`), `onTrap` keeps itself.
   `onTrap` is given what `mscratch` holds (the kernel's state, set with
   `machine_set_trap_state`, and `null` until then), the cause and the address of the instruction
   the trap stopped. */
__attribute__((naked, used, aligned(4))) static void machine_trap_vector(void) {
    __asm__ volatile(
        "addi sp, sp, -304\n"
        "sd ra, 0(sp)\n"   "sd t0, 8(sp)\n"   "sd t1, 16(sp)\n"  "sd t2, 24(sp)\n"
        "sd t3, 32(sp)\n"  "sd t4, 40(sp)\n"  "sd t5, 48(sp)\n"  "sd t6, 56(sp)\n"
        "sd a0, 64(sp)\n"  "sd a1, 72(sp)\n"  "sd a2, 80(sp)\n"  "sd a3, 88(sp)\n"
        "sd a4, 96(sp)\n"  "sd a5, 104(sp)\n" "sd a6, 112(sp)\n" "sd a7, 120(sp)\n"
        MACHINE_FP_SAVE
        "csrr a0, mscratch\n"
        "csrr a1, mcause\n"
        "csrr a2, mepc\n"
        "call onTrap\n"
        MACHINE_FP_LOAD
        "ld ra, 0(sp)\n"   "ld t0, 8(sp)\n"   "ld t1, 16(sp)\n"  "ld t2, 24(sp)\n"
        "ld t3, 32(sp)\n"  "ld t4, 40(sp)\n"  "ld t5, 48(sp)\n"  "ld t6, 56(sp)\n"
        "ld a0, 64(sp)\n"  "ld a1, 72(sp)\n"  "ld a2, 80(sp)\n"  "ld a3, 88(sp)\n"
        "ld a4, 96(sp)\n"  "ld a5, 104(sp)\n" "ld a6, 112(sp)\n" "ld a7, 120(sp)\n"
        "addi sp, sp, 304\n"
        "mret\n");
}

/* The address the vector hands `onTrap`. */
static inline void machine_set_trap_state(void* state) {
    __asm__ volatile("csrw mscratch, %0" : : "r"(state) : "memory");
}

/* The timer's interrupt (bit 7 of `mie`) and interrupts at all (bit 3 of `mstatus`). */
static inline void machine_enable_timer(void) {
    __asm__ volatile("csrs mie, %0" : : "r"((uintptr_t)1 << 7));
    __asm__ volatile("csrs mstatus, %0" : : "r"((uintptr_t)1 << 3) : "memory");
}

static inline void machine_disable_interrupts(void) {
    __asm__ volatile("csrc mstatus, %0" : : "r"((uintptr_t)1 << 3) : "memory");
}

/* The core sleeps until an interrupt is pending. */
static inline void machine_wait(void) {
    __asm__ volatile("wfi" : : : "memory");
}

/* The memory the linker script leaves for the heap, and the word the tests leave. */
static inline uint8_t* machine_heap_start(void) {
    return (uint8_t*)__heap_start;
}

static inline size_t machine_heap_size(void) {
    return (size_t)(__heap_end - __heap_start);
}

static inline uint32_t* machine_asked(void) {
    return (uint32_t*)__asked;
}
