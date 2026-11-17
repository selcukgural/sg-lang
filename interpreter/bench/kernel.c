/* The kernel of `examples/kernel` as C would write it, for its size: the same start, vector of
   traps and registers of the core (its `machine.c`, included), the same devices driven through
   `volatile`, numbers written into a buffer by hand, no heap and no checks. It writes what the
   SG kernel writes but the line about the heap's blocks, which it has none of. */
#include <stddef.h>
#include <stdint.h>

void kernelMain(void);
void onTrap(void* state, uint64_t cause, uint64_t pc);

#include "../examples/kernel/machine.c"

static volatile uint8_t* const uart = (volatile uint8_t*)0x10000000;
static volatile uint64_t* const mtime = (volatile uint64_t*)0x0200bff8;
static volatile uint64_t* const mtimecmp = (volatile uint64_t*)0x02004000;
static volatile uint32_t* const test_device = (volatile uint32_t*)0x00100000;

static void put(const char* text) {
    for (; *text; text++) {
        while ((uart[5] & 0x20) == 0) { }
        uart[0] = (uint8_t)*text;
    }
}

static void put_number(uint64_t n, int base) {
    char digits[24];
    int at = (int)sizeof digits - 1;
    digits[at] = 0;
    do {
        digits[--at] = "0123456789abcdef"[n % (uint64_t)base];
        n /= (uint64_t)base;
    } while (n);
    put(&digits[at]);
}

static void power_off(int code) {
    *test_device = code == 0 ? 0x5555 : (uint32_t)code << 16 | 0x3333;
}

struct kernel {
    uint64_t ticks;
    uint64_t interval;
};

static void start_next(uint64_t interval) {
    *mtimecmp = *mtime + interval;
}

void kernelMain(void) {
    uart[1] = 0x00;
    uart[3] = 0x03;
    uart[2] = 0x07;
    put("SG on RISC-V: a kernel in machine mode on QEMU's virt machine\n");
    put("the heap has ");
    put_number(machine_heap_size() / 1024 / 1024, 10);
    put(" MB\n");
    if (*(volatile uint32_t*)machine_asked() == 1) {
        put("reading where nothing is\n");
        (void)*(volatile uint32_t*)0x90000000;
    }
    struct kernel kernel = { 0, 100000 };
    machine_set_trap_state(&kernel);
    start_next(kernel.interval);
    machine_enable_timer();
    put("the timer interrupts every 10 ms\n");
    uint64_t seen = 0;
    while (seen < 5) {
        machine_wait();
        uint64_t now = __atomic_load_n(&kernel.ticks, __ATOMIC_SEQ_CST);
        while (seen < now && seen < 5) {
            seen++;
            put("tick ");
            put_number(seen, 10);
            put("\n");
        }
    }
    machine_disable_interrupts();
    put("powering off\n");
    power_off(0);
}

void onTrap(void* state, uint64_t cause, uint64_t pc) {
    if (cause == ((uint64_t)1 << 63 | 7)) {
        struct kernel* kernel = state;
        __atomic_fetch_add(&kernel->ticks, 1, __ATOMIC_SEQ_CST);
        start_next(kernel->interval);
        return;
    }
    put("panic: unexpected trap: ");
    put(cause == 2 ? "an illegal instruction" : cause == 5 ? "a fault of a load" : cause == 7 ? "a fault of a store" : "a trap");
    put(" at 0x");
    put_number(pc, 16);
    put("\n");
    power_off(101);
    for (;;) { }
}
