/* `cat`, for the programs that start it (`capture`, `pipelines`, `pipesasync`) on Windows, which
   has none: each file of its command line to the output, as bytes, in blocks of 64 KB. `run.py`
   builds it into `target/bench/tools`, first on `PATH`, with `true`. */
#include <fcntl.h>
#include <io.h>
#include <stdio.h>

int main(int argc, char** argv) {
    _setmode(_fileno(stdout), _O_BINARY);
    static char block[65536];
    for (int i = 1; i < argc; i++) {
        FILE* in = fopen(argv[i], "rb");
        if (!in) {
            perror(argv[i]);
            return 1;
        }
        size_t got;
        while ((got = fread(block, 1, sizeof block, in)) > 0) {
            if (fwrite(block, 1, got, stdout) != got) return 1;
        }
        fclose(in);
    }
    return fflush(stdout) == 0 ? 0 : 1;
}
