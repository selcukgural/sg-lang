// The same program written by hand: a linked list built, walked and freed.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct Node {
    int64_t value;
    struct Node* next;
} Node;

static int64_t round_of(int32_t count) {
    Node* list = NULL;
    for (int32_t i = 0; i < count; i++) {
        Node* node = malloc(sizeof(Node));
        if (!node) abort();
        node->value = i;
        node->next = list;
        list = node;
    }
    int64_t total = 0;
    for (const Node* cur = list; cur; cur = cur->next) {
        total += cur->value;
    }
    while (list) {
        Node* next = list->next;
        free(list);
        list = next;
    }
    return total;
}

int main(void) {
    int64_t sum = 0;
    for (int32_t i = 0; i < 20; i++) {
        sum += round_of(1000000);
    }
    printf("%lld\n", (long long)sum);
    return 0;
}
