/* The headers of ICU4C that `build.zig` translates for `unicode.zig`: the general category of a
   code point, the normal forms, and UTF-8 turned into UTF-16 and back. ICU names its functions
   with its version (`u_charType` is `u_charType_79`) by a macro Zig cannot translate, so each
   function `unicode.zig` calls is given here under a name of its own, the same in every version. */
#include <unicode/uchar.h>
#include <unicode/unorm2.h>
#include <unicode/ustring.h>

static inline int8_t icu_charType(UChar32 c) {
    return u_charType(c);
}

static inline UChar *icu_strFromUTF8(UChar *dest, int32_t capacity, int32_t *length, const char *src,
                                     int32_t srcLength, UErrorCode *status) {
    return u_strFromUTF8(dest, capacity, length, src, srcLength, status);
}

static inline char *icu_strToUTF8(char *dest, int32_t capacity, int32_t *length, const UChar *src,
                                  int32_t srcLength, UErrorCode *status) {
    return u_strToUTF8(dest, capacity, length, src, srcLength, status);
}

static inline const UNormalizer2 *icu_nfc(UErrorCode *status) {
    return unorm2_getNFCInstance(status);
}

static inline const UNormalizer2 *icu_nfd(UErrorCode *status) {
    return unorm2_getNFDInstance(status);
}

static inline int32_t icu_normalize(const UNormalizer2 *form, const UChar *src, int32_t length, UChar *dest,
                                    int32_t capacity, UErrorCode *status) {
    return unorm2_normalize(form, src, length, dest, capacity, status);
}

static inline UBool icu_isNormalized(const UNormalizer2 *form, const UChar *s, int32_t length, UErrorCode *status) {
    return unorm2_isNormalized(form, s, length, status);
}
