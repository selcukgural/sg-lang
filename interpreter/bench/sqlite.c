/* link: -lsqlite3 */
/* The same program written by hand in C, calling SQLite through its header. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sqlite3.h>

static sqlite3_stmt* prepared(sqlite3* db, const char* sql) {
    sqlite3_stmt* stmt = NULL;
    if (sqlite3_prepare_v2(db, sql, -1, &stmt, NULL) != SQLITE_OK) {
        fprintf(stderr, "not prepared\n");
        exit(1);
    }
    return stmt;
}

int main(void) {
    sqlite3* db = NULL;
    if (sqlite3_open(":memory:", &db) != SQLITE_OK) {
        fprintf(stderr, "not opened\n");
        return 1;
    }
    sqlite3_exec(db, "CREATE TABLE t (n INTEGER, word TEXT)", NULL, NULL, NULL);
    sqlite3_exec(db, "BEGIN", NULL, NULL, NULL);
    sqlite3_stmt* insert = prepared(db, "INSERT INTO t VALUES (?1, ?2)");
    char word[32];
    for (int32_t i = 0; i < 1000000; i++) {
        int64_t n = (int64_t)i * 7 % 1000;
        int length = snprintf(word, sizeof word, "item-%ld", (long)n);
        sqlite3_bind_int64(insert, 1, n);
        sqlite3_bind_text(insert, 2, word, length, SQLITE_TRANSIENT);
        if (sqlite3_step(insert) != SQLITE_DONE) {
            fprintf(stderr, "not inserted\n");
            return 1;
        }
        sqlite3_reset(insert);
    }
    sqlite3_finalize(insert);
    sqlite3_exec(db, "COMMIT", NULL, NULL, NULL);
    sqlite3_stmt* select = prepared(db, "SELECT n, word FROM t");
    int64_t total = 0, bytes = 0;
    while (sqlite3_step(select) == SQLITE_ROW) {
        total += sqlite3_column_int64(select, 0);
        bytes += sqlite3_column_bytes(select, 1);
    }
    sqlite3_finalize(select);
    sqlite3_close(db);
    printf("%lld %lld\n", (long long)total, (long long)bytes);
    return 0;
}
