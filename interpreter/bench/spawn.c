/* The same program written by hand in C: `posix_spawnp`, which looks in `PATH` too, and `waitpid`. */
#include <spawn.h>
#include <stdio.h>
#include <sys/wait.h>

extern char** environ;

int main(void) {
    char* argv[] = { "true", NULL };
    long ran = 0;
    for (int i = 0; i < 5000; i++) {
        pid_t child;
        if (posix_spawnp(&child, "true", NULL, NULL, argv, environ) != 0) return 1;
        int status;
        if (waitpid(child, &status, 0) < 0 || !WIFEXITED(status) || WEXITSTATUS(status) != 0) return 1;
        ran++;
    }
    printf("%ld\n", ran);
    return 0;
}
