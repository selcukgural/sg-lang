/* The same program written by hand in C: the C library's wide characters in a UTF-8 locale,
   `mbrtowc`, `towupper` and `wcrtomb`. They map one character to one (`ß` stays `ß`), so the
   lengths differ a little from SG's. */
#include <locale.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>
#include <wctype.h>

static char* changed(const char* text, size_t len, int upper, size_t* out_len) {
    char* out = malloc(len * 2 + 8);
    size_t at = 0, put = 0;
    mbstate_t in_state, out_state;
    memset(&in_state, 0, sizeof in_state);
    memset(&out_state, 0, sizeof out_state);
    while (at < len) {
        wchar_t c;
        size_t n = mbrtowc(&c, text + at, len - at, &in_state);
        if (n == 0 || n > len - at) n = 1;
        at += n;
        c = upper ? (wchar_t)towupper((wint_t)c) : (wchar_t)towlower((wint_t)c);
        put += wcrtomb(out + put, c, &out_state);
    }
    *out_len = put;
    return out;
}

int main(void) {
    if (!setlocale(LC_CTYPE, "C.UTF-8") && !setlocale(LC_CTYPE, "en_US.UTF-8")) return 1;
    const char* line = "İstanbul'da ılık bir sabah, Größe der Straße; ΟΔΟΣ Ελληνικά, café naïve, Привет мир, the quick brown fox jumps over the lazy dog 日本語 ";
    size_t line_len = strlen(line), len = line_len * 8000;
    char* text = malloc(len);
    for (size_t i = 0; i < 8000; i++) memcpy(text + i * line_len, line, line_len);
    size_t total = 0;
    for (int round = 0; round < 50; round++) {
        size_t upper_len, lower_len;
        char* upper = changed(text, len, 1, &upper_len);
        char* lower = changed(upper, upper_len, 0, &lower_len);
        total += upper_len + lower_len;
        free(upper);
        free(lower);
    }
    printf("%zu bytes, %zu written\n", len, total);
    free(text);
    return 0;
}
