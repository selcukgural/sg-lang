/* The same program written by hand in C: `posix_spawnp` with the output into a pipe, read into a
   block that grows; no check of the text. */
#include <spawn.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

extern char** environ;

int main(void) {
    char* argv[] = { "cat", "target/bench/lines.in", NULL };
    long long total = 0;
    for (int i = 0; i < 5; i++) {
        int ends[2];
        if (pipe(ends) != 0) return 1;
        posix_spawn_file_actions_t actions;
        posix_spawn_file_actions_init(&actions);
        posix_spawn_file_actions_adddup2(&actions, ends[1], 1);
        posix_spawn_file_actions_addclose(&actions, ends[0]);
        pid_t child;
        if (posix_spawnp(&child, "cat", &actions, NULL, argv, environ) != 0) return 1;
        posix_spawn_file_actions_destroy(&actions);
        close(ends[1]);
        size_t len = 0, cap = 65536;
        char* text = malloc(cap);
        if (!text) return 1;
        for (;;) {
            if (cap - len < 65536) {
                cap *= 2;
                text = realloc(text, cap);
                if (!text) return 1;
            }
            ssize_t got = read(ends[0], text + len, cap - len);
            if (got < 0) return 1;
            if (got == 0) break;
            len += (size_t)got;
        }
        close(ends[0]);
        int status;
        if (waitpid(child, &status, 0) < 0 || !WIFEXITED(status) || WEXITSTATUS(status) != 0) return 1;
        total += (long long)len;
        free(text);
    }
    printf("%lld\n", total);
    return 0;
}
