/* The same program written by hand in C: the C library's wide characters in a UTF-8 locale,
   `mbrtowc`, `iswalpha`, `iswupper` and `iswdigit` (whose answers are the C library's, not
   always Unicode's categories). */
#include <locale.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>
#include <wctype.h>

int main(void) {
    if (!setlocale(LC_CTYPE, "C.UTF-8") && !setlocale(LC_CTYPE, "en_US.UTF-8")) return 1;
    const char* line = "İstanbul'da ılık bir sabah, Größe der Straße; ΟΔΟΣ Ελληνικά, café naïve, Привет мир, the quick brown fox jumps over the lazy dog 日本語 ٣ ";
    size_t line_len = strlen(line), len = line_len * 8000;
    char* text = malloc(len);
    for (size_t i = 0; i < 8000; i++) memcpy(text + i * line_len, line, line_len);
    long letters = 0, uppers = 0, numbers = 0;
    for (int round = 0; round < 200; round++) {
        mbstate_t state;
        memset(&state, 0, sizeof state);
        for (size_t at = 0; at < len; ) {
            wchar_t c;
            size_t n = mbrtowc(&c, text + at, len - at, &state);
            if (n == 0 || n > len - at) n = 1;
            at += n;
            if (iswalpha((wint_t)c)) letters++;
            if (iswupper((wint_t)c)) uppers++;
            if (iswdigit((wint_t)c)) numbers++;
        }
    }
    printf("%ld letters, %ld upper case, %ld numbers\n", letters, uppers, numbers);
    free(text);
    return 0;
}
