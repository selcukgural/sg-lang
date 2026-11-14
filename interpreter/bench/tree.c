// The same program written by hand: a folder points at its parent and owns an array of
// pointers to its subfolders; the tree is freed from the root down when the round ends.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct Folder {
    int64_t bytes;
    struct Folder* parent;
    struct Folder** children;
    int64_t len, cap;
} Folder;

static Folder* make(Folder* parent) {
    Folder* f = malloc(sizeof(Folder));
    if (!f) abort();
    *f = (Folder){ 0, parent, NULL, 0, 0 };
    return f;
}

static Folder* add(Folder* f) {
    Folder* created = make(f);
    if (f->len == f->cap) {
        f->cap = f->cap ? f->cap * 2 : 4;
        f->children = realloc(f->children, sizeof(Folder*) * (size_t)f->cap);
        if (!f->children) abort();
    }
    f->children[f->len++] = created;
    return created;
}

static void add_bytes(Folder* f, int64_t amount) {
    for (Folder* cur = f; cur; cur = cur->parent) cur->bytes += amount;
}

static void grow(Folder* folder, int depth, int64_t* made) {
    if (depth == 0) return;
    for (int i = 0; i < 10; i++) {
        Folder* child = add(folder);
        *made += 1;
        add_bytes(child, *made % 1000);
        grow(child, depth - 1, made);
    }
}

static void release(Folder* f) {
    for (int64_t i = 0; i < f->len; i++) release(f->children[i]);
    free(f->children);
    free(f);
}

int main(void) {
    int64_t total = 0, made = 0;
    for (int round = 0; round < 20; round++) {
        Folder* root = make(NULL);
        grow(root, 5, &made);
        total += root->bytes;
        release(root);
    }
    printf("%lld\n", (long long)total);
    return 0;
}
