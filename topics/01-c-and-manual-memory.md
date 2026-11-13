# C and manual memory management

Why does C leave memory management to the programmer? The reason lies in the context C was designed in: it appeared in 1972 as "portable assembly" for writing Unix. The intent was to have no invisible runtime layer on top of the hardware.

The concrete reasons behind this:

- **Zero-cost abstraction.** Everything the program does corresponds directly to what is written in the code. No garbage collector that you did not write runs in the background, and there are no object headers, reference counters or stop-the-world pauses. You know exactly how much memory you use and when it is freed.

- **Predictability.** In kernels, drivers, embedded systems and real-time software, "the GC paused for 30 ms just now" is not acceptable. When memory is taken and when it is released has to be deterministic.

- **No runtime dependency.** A GC is itself a program that has to be written, and it needs memory management underneath. The language used to write an operating system kernel cannot itself depend on that infrastructure. C is the language used to write its own infrastructure.

- **Control over hardware and allocation strategy.** Stack, heap, static storage, an arena or pool allocator you wrote yourself, a DMA buffer, a memory-mapped hardware register: all are expressed with the same pointer model. The language tells you "memory is an array of bytes, the rest is your job".

- **The hardware of the 1970s.** On a PDP-11, with memory measured in kilobytes, the overhead of a GC counted as a luxury. Languages with a GC, such as Lisp, existed at the time, but they were too heavy for systems programming.

In short, the purpose was not "to torment the developer" but **to separate mechanism from policy**: the language provides only the mechanism, and the owner of the program sets the policy. The price was the well-known bugs: use-after-free, double free, leaks, buffer overflow. Rust's ownership model is exactly an attempt to keep the same control without paying this price.
