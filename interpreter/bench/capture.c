/* The same program written by hand in C: `posix_spawnp` with the output into a pipe, read into a
   block that grows; no check of the text. */
#include <stdio.h>
#include <stdlib.h>
#if defined(_WIN32)
#define WIN32_LEAN_AND_MEAN
#include <windows.h>

/* Windows: `CreateProcessW` with the output into a pipe of `CreatePipe`, read with `ReadFile`. */
int main(void) {
    long long total = 0;
    for (int i = 0; i < 5; i++) {
        SECURITY_ATTRIBUTES inherited = { sizeof inherited, NULL, TRUE };
        HANDLE ours, theirs;
        if (!CreatePipe(&ours, &theirs, &inherited, 0)) return 1;
        SetHandleInformation(ours, HANDLE_FLAG_INHERIT, 0);
        wchar_t line[] = L"cat target/bench/lines.in";
        STARTUPINFOW start = { .cb = sizeof start, .dwFlags = STARTF_USESTDHANDLES };
        start.hStdInput = GetStdHandle(STD_INPUT_HANDLE);
        start.hStdOutput = theirs;
        start.hStdError = GetStdHandle(STD_ERROR_HANDLE);
        PROCESS_INFORMATION child;
        if (!CreateProcessW(NULL, line, NULL, NULL, TRUE, 0, NULL, NULL, &start, &child)) return 1;
        CloseHandle(child.hThread);
        CloseHandle(theirs);
        size_t len = 0, cap = 65536;
        char* text = malloc(cap);
        if (!text) return 1;
        for (;;) {
            if (cap - len < 65536) {
                cap *= 2;
                text = realloc(text, cap);
                if (!text) return 1;
            }
            DWORD got = 0;
            if (!ReadFile(ours, text + len, (DWORD)(cap - len), &got, NULL)) {
                if (GetLastError() == ERROR_BROKEN_PIPE) break;
                return 1;
            }
            if (got == 0) break;
            len += got;
        }
        CloseHandle(ours);
        DWORD code = 1;
        if (WaitForSingleObject(child.hProcess, INFINITE) != WAIT_OBJECT_0 || !GetExitCodeProcess(child.hProcess, &code) || code != 0) return 1;
        CloseHandle(child.hProcess);
        total += (long long)len;
        free(text);
    }
    printf("%lld\n", total);
    return 0;
}
#else
#include <spawn.h>
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
#endif
