/* The same program written by hand in C, with no checks. The numbers are written as SG
   writes them: the fewest digits that read back as the same number. Compiled, like SG's
   code, without fused multiply-add (`-ffp-contract=off` for gcc; the pragma for clang). */
#include <math.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(__clang__)
#pragma STDC FP_CONTRACT OFF
#endif

typedef struct { double x, y, z, vx, vy, vz, mass; } Body;

static void advance(Body* bodies, int count, double dt) {
    for (int i = 0; i < count; i++) {
        for (int j = i + 1; j < count; j++) {
            double dx = bodies[i].x - bodies[j].x;
            double dy = bodies[i].y - bodies[j].y;
            double dz = bodies[i].z - bodies[j].z;
            double squared = dx * dx + dy * dy + dz * dz;
            double distance = sqrt(squared);
            double magnitude = dt / (squared * distance);
            double mi = bodies[i].mass * magnitude;
            double mj = bodies[j].mass * magnitude;
            bodies[i].vx -= dx * mj;
            bodies[i].vy -= dy * mj;
            bodies[i].vz -= dz * mj;
            bodies[j].vx += dx * mi;
            bodies[j].vy += dy * mi;
            bodies[j].vz += dz * mi;
        }
    }
    for (int i = 0; i < count; i++) {
        bodies[i].x += dt * bodies[i].vx;
        bodies[i].y += dt * bodies[i].vy;
        bodies[i].z += dt * bodies[i].vz;
    }
}

static double energy(const Body* bodies, int count) {
    double e = 0;
    for (int i = 0; i < count; i++) {
        Body b = bodies[i];
        e += 0.5 * b.mass * (b.vx * b.vx + b.vy * b.vy + b.vz * b.vz);
        for (int j = i + 1; j < count; j++) {
            double dx = b.x - bodies[j].x;
            double dy = b.y - bodies[j].y;
            double dz = b.z - bodies[j].z;
            e -= b.mass * bodies[j].mass / sqrt(dx * dx + dy * dy + dz * dz);
        }
    }
    return e;
}

/* The fewest digits that read back as the same number, as SG prints it (no number here
   needs an exponent). */
static void print_shortest(double v) {
    char text[40];
    for (int p = 1; p <= 17; p++) {
        snprintf(text, sizeof text, "%.*g", p, v);
        if (strtod(text, NULL) == v) break;
    }
    puts(text);
}

int main(void) {
    double pi = 3.141592653589793;
    double mass = 4 * pi * pi;
    double year = 365.24;
    Body bodies[5] = {
        { 0, 0, 0, 0, 0, 0, mass },
        { 4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01,
          1.66007664274403694e-03 * year, 7.69901118419740425e-03 * year, -6.90460016972063023e-05 * year,
          9.54791938424326609e-04 * mass },
        { 8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01,
          -2.76742510726862411e-03 * year, 4.99852801234917238e-03 * year, 2.30417297573763929e-05 * year,
          2.85885980666130812e-04 * mass },
        { 1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01,
          2.96460137564761618e-03 * year, 2.37847173959480950e-03 * year, -2.96589568540237556e-05 * year,
          4.36624404335156298e-05 * mass },
        { 1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01,
          2.68067772490389322e-03 * year, 1.62824170038242295e-03 * year, -9.51592254519715870e-05 * year,
          5.15138902046611451e-05 * mass },
    };
    double px = 0, py = 0, pz = 0;
    for (int i = 0; i < 5; i++) {
        px += bodies[i].vx * bodies[i].mass;
        py += bodies[i].vy * bodies[i].mass;
        pz += bodies[i].vz * bodies[i].mass;
    }
    bodies[0].vx = -px / mass;
    bodies[0].vy = -py / mass;
    bodies[0].vz = -pz / mass;

    print_shortest(energy(bodies, 5));
    for (int step = 0; step < 20000000; step++) {
        advance(bodies, 5, 0.01);
    }
    print_shortest(energy(bodies, 5));
    return 0;
}
