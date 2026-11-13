/* The same lines as show.sg, written with printf. */
#include <stdio.h>

typedef struct { int x, y; } Point;

int main(void) {
    int xs[8];
    for (int i = 0; i < 8; i++) xs[i] = i * 3;
    for (int i = 0; i < 2000000; i++) {
        Point p = { i, i % 97 };
        printf("Point { x = %d, y = %d }\n", p.x, p.y);
        if (i % 4 == 0) {
            printf("[");
            for (int k = 0; k < 8; k++) printf(k ? ", %d" : "%d", xs[k]);
            printf("]\n");
        }
    }
    return 0;
}
