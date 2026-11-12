// The same program written by hand: a tagged union, a switch on the tag, malloc and free.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef enum { NUMBER, NEGATE, ADD, MULTIPLY } Kind;

typedef struct Expr {
    uint8_t kind;
    union {
        int64_t value;
        struct Expr* operand;
        struct { struct Expr* left; struct Expr* right; } pair;
    } u;
} Expr;

static Expr* make(uint8_t kind) {
    Expr* e = malloc(sizeof(Expr));
    if (!e) abort();
    e->kind = kind;
    return e;
}

static Expr* build(int32_t depth, int64_t seed) {
    if (depth == 0) {
        Expr* e = make(NUMBER);
        e->u.value = seed % 7;
        return e;
    }
    int64_t kind = seed % 3;
    if (kind == 0) {
        Expr* operand = build(depth - 1, (seed * 31 + 1) % 1000003);
        Expr* e = make(NEGATE);
        e->u.operand = operand;
        return e;
    }
    Expr* left = build(depth - 1, (seed * 17 + 3) % 1000003);
    Expr* right = build(depth - 1, (seed * 13 + 5) % 1000003);
    Expr* e = make(kind == 1 ? ADD : MULTIPLY);
    e->u.pair.left = left;
    e->u.pair.right = right;
    return e;
}

static int64_t eval(const Expr* e, int64_t bias) {
    switch (e->kind) {
    case NUMBER: return e->u.value + bias;
    case NEGATE: return -eval(e->u.operand, bias);
    case ADD: return (eval(e->u.pair.left, bias) + eval(e->u.pair.right, bias)) % 1000003;
    default: return (eval(e->u.pair.left, bias) * eval(e->u.pair.right, bias)) % 1000003;
    }
}

// Like the SG program, a node that is taken apart is freed and its replacement allocated.
// (By hand one could also rewrite the node in place; SG has no notation for that yet.)
static Expr* simplify(Expr* e) {
    switch (e->kind) {
    case NEGATE: {
        Expr* inner = simplify(e->u.operand);
        free(e);
        if (inner->kind == NEGATE) {
            Expr* twice = inner->u.operand;
            free(inner);
            return twice;
        }
        Expr* out = make(NEGATE);
        out->u.operand = inner;
        return out;
    }
    case ADD:
    case MULTIPLY: {
        Expr* left = e->u.pair.left;
        Expr* right = e->u.pair.right;
        uint8_t kind = e->kind;
        free(e);
        left = simplify(left);
        right = simplify(right);
        Expr* out = make(kind);
        out->u.pair.left = left;
        out->u.pair.right = right;
        return out;
    }
    default:
        return e;
    }
}

static void drop(Expr* e) {
    switch (e->kind) {
    case NEGATE: drop(e->u.operand); break;
    case ADD:
    case MULTIPLY: drop(e->u.pair.left); drop(e->u.pair.right); break;
    default: break;
    }
    free(e);
}

int main(void) {
    int64_t total = 0;
    for (int32_t round = 0; round < 3; round++) {
        Expr* tree = build(25, round + 1);
        for (int32_t i = 0; i < 20; i++) {
            total += eval(tree, i);
        }
        Expr* simple = simplify(tree);
        total += eval(simple, 0);
        drop(simple);
    }
    printf("%lld\n", (long long)total);
    return 0;
}
