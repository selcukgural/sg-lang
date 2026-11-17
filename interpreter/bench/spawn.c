/* The same program written by hand in C: `posix_spawnp`, which looks in `PATH` too, and `waitpid`. */
#include <stdio.h>
#if defined(_WIN32)
#define WIN32_LEAN_AND_MEAN
#include <windows.h>

/* Windows: `CreateProcessW` given only a command line, which looks in `PATH` too, and
   `WaitForSingleObject`. */
int main(void) {
    long ran = 0;
    for (int i = 0; i < 5000; i++) {
        wchar_t line[] = L"true";
        STARTUPINFOW start = { .cb = sizeof start };
        PROCESS_INFORMATION child;
        if (!CreateProcessW(NULL, line, NULL, NULL, FALSE, 0, NULL, NULL, &start, &child)) return 1;
        CloseHandle(child.hThread);
        DWORD code = 1;
        if (WaitForSingleObject(child.hProcess, INFINITE) != WAIT_OBJECT_0 || !GetExitCodeProcess(child.hProcess, &code) || code != 0) return 1;
        CloseHandle(child.hProcess);
        ran++;
    }
    printf("%ld\n", ran);
    return 0;
}
#else
#include <spawn.h>
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
#endif
