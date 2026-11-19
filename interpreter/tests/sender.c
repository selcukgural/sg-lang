// The sender of `shutdown_on_signals` on Windows (tests/golden.rs), called two ways:
//
//   sender <pid> 0|1         attaches to the console of program <pid> and raises Ctrl-C (0) or
//                            Ctrl-Break (1) there, so the tests do not lose their own console;
//   sender pty <command line>
//                            runs the command line in a pseudo console of its own, with the
//                            sender's standard output and error, and says first `server <pid>`.
//                            A line (or the end) of its standard input closes that console, as
//                            closing a tab of Windows Terminal does; it then ends with the
//                            program's exit code.
//   sender show <command line>
//                            runs the command line in a pseudo console of its own whose output
//                            and error are that console, and writes what the console shows (in
//                            VT codes) to the sender's output; ends with the program's exit
//                            code. `Log` colours its levels there (choice 1455).
//
// A console made with CREATE_NEW_CONSOLE and closed through its window is not closed the same
// way on every machine: where new consoles go to a terminal of their own, the window is only a
// stand-in (`PseudoConsoleWindow`), and a WM_CLOSE posted to it closes nothing (GitHub's machine
// of x86-64). A pseudo console is closed the same way everywhere, and the program gets the same
// CTRL_CLOSE_EVENT.
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static BOOL WINAPI kept(DWORD event) {
    (void)event;
    return TRUE;
}

// Where `show` copies what the pseudo console shows; NULL for `pty`.
static HANDLE shown_to = NULL;

// What the pseudo console shows (in VT codes) is read, and copied for `show` or else dropped,
// so it never waits on a full pipe.
static DWORD WINAPI drain(LPVOID pipe) {
    char buffer[4096];
    DWORD got, put;
    while (ReadFile((HANDLE)pipe, buffer, sizeof buffer, &got, NULL) && got > 0) {
        if (shown_to) WriteFile(shown_to, buffer, got, &put, NULL);
    }
    return 0;
}

static int show(wchar_t* line) {
    HANDLE in_read, in_write, out_read, out_write;
    if (!CreatePipe(&in_read, &in_write, NULL, 0) || !CreatePipe(&out_read, &out_write, NULL, 0)) return 10;
    HPCON console;
    COORD size = { 200, 50 };
    if (CreatePseudoConsole(size, in_read, out_write, 0, &console) != S_OK) return 11;
    shown_to = GetStdHandle(STD_OUTPUT_HANDLE);
    HANDLE drainer = CreateThread(NULL, 0, drain, out_read, 0, NULL);
    if (!drainer) return 12;
    SIZE_T bytes = 0;
    InitializeProcThreadAttributeList(NULL, 1, 0, &bytes);
    LPPROC_THREAD_ATTRIBUTE_LIST list = (LPPROC_THREAD_ATTRIBUTE_LIST)HeapAlloc(GetProcessHeap(), 0, bytes);
    if (!list || !InitializeProcThreadAttributeList(list, 1, 0, &bytes)) return 13;
    if (!UpdateProcThreadAttribute(list, 0, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, console, sizeof console, NULL, NULL)) return 14;
    // No handles of the sender: the program's output and error are the pseudo console. A child
    // takes the standard handles of its parent when they are not those of a console, so the
    // sender lets go of its own first (it writes through `shown_to`).
    SetStdHandle(STD_INPUT_HANDLE, NULL);
    SetStdHandle(STD_OUTPUT_HANDLE, NULL);
    SetStdHandle(STD_ERROR_HANDLE, NULL);
    STARTUPINFOEXW info;
    memset(&info, 0, sizeof info);
    info.StartupInfo.cb = sizeof info;
    info.lpAttributeList = list;
    PROCESS_INFORMATION program;
    if (!CreateProcessW(NULL, line, NULL, NULL, FALSE, EXTENDED_STARTUPINFO_PRESENT, NULL, NULL, &info.StartupInfo, &program)) return 16;
    CloseHandle(in_read);
    CloseHandle(out_write);
    WaitForSingleObject(program.hProcess, INFINITE);
    DWORD code = 1;
    GetExitCodeProcess(program.hProcess, &code);
    // What the console still holds is shown before it closes.
    Sleep(500);
    ClosePseudoConsole(console);
    WaitForSingleObject(drainer, 5000);
    CloseHandle(in_write);
    return (int)code;
}

static int in_pty(wchar_t* line) {
    HANDLE in_read, in_write, out_read, out_write;
    if (!CreatePipe(&in_read, &in_write, NULL, 0) || !CreatePipe(&out_read, &out_write, NULL, 0)) return 10;
    HPCON console;
    COORD size = { 80, 25 };
    if (CreatePseudoConsole(size, in_read, out_write, 0, &console) != S_OK) return 11;
    HANDLE drainer = CreateThread(NULL, 0, drain, out_read, 0, NULL);
    if (!drainer) return 12;

    SIZE_T bytes = 0;
    InitializeProcThreadAttributeList(NULL, 1, 0, &bytes);
    LPPROC_THREAD_ATTRIBUTE_LIST list = (LPPROC_THREAD_ATTRIBUTE_LIST)HeapAlloc(GetProcessHeap(), 0, bytes);
    if (!list || !InitializeProcThreadAttributeList(list, 1, 0, &bytes)) return 13;
    if (!UpdateProcThreadAttribute(list, 0, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, console, sizeof console, NULL, NULL)) return 14;

    // The program writes where the sender writes; the sender's input stays the sender's.
    HANDLE out = GetStdHandle(STD_OUTPUT_HANDLE), err = GetStdHandle(STD_ERROR_HANDLE), in = GetStdHandle(STD_INPUT_HANDLE);
    SetHandleInformation(out, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT);
    SetHandleInformation(err, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT);
    SetHandleInformation(in, HANDLE_FLAG_INHERIT, 0);
    STARTUPINFOEXW info;
    memset(&info, 0, sizeof info);
    info.StartupInfo.cb = sizeof info;
    info.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    info.StartupInfo.hStdOutput = out;
    info.StartupInfo.hStdError = err;
    info.lpAttributeList = list;

    // In a job that ends with the sender: a sender stopped by the tests takes the program with it.
    HANDLE job = CreateJobObjectW(NULL, NULL);
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits;
    memset(&limits, 0, sizeof limits);
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if (!job || !SetInformationJobObject(job, JobObjectExtendedLimitInformation, &limits, sizeof limits)) return 15;

    PROCESS_INFORMATION program;
    if (!CreateProcessW(NULL, line, NULL, NULL, TRUE, EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED, NULL, NULL, &info.StartupInfo, &program)) return 16;
    CloseHandle(in_read);
    CloseHandle(out_write);
    if (!AssignProcessToJobObject(job, program.hProcess)) {
        TerminateProcess(program.hProcess, 1);
        return 17;
    }
    // Said before the program runs, so it is the first line.
    printf("server %lu\n", (unsigned long)program.dwProcessId);
    fflush(stdout);
    ResumeThread(program.hThread);

    char word[64];
    DWORD got;
    ReadFile(in, word, sizeof word, &got, NULL);
    ClosePseudoConsole(console);
    WaitForSingleObject(program.hProcess, INFINITE);
    DWORD code = 1;
    GetExitCodeProcess(program.hProcess, &code);
    CloseHandle(in_write);
    WaitForSingleObject(drainer, 5000);
    return (int)code;
}

int wmain(int argc, wchar_t** argv) {
    if (argc == 3 && wcscmp(argv[1], L"pty") == 0) return in_pty(argv[2]);
    if (argc == 3 && wcscmp(argv[1], L"show") == 0) return show(argv[2]);
    if (argc != 3) return 2;
    FreeConsole();
    if (!AttachConsole((DWORD)_wtoi(argv[1]))) return 3;
    SetConsoleCtrlHandler(kept, TRUE);
    return GenerateConsoleCtrlEvent((DWORD)_wtoi(argv[2]), 0) ? 0 : 4;
}
