/* Variables of C that `main.sg` reads and writes, compiled with it, on every system. */
const char* greeting = "hello from C";
int counter = 41;
double scale[3] = { 0.5, 1.5, 2.5 };

/* What C sees of the counter after SG has written it. */
int bumped(void) {
    return ++counter;
}
