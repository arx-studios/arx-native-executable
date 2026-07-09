/* ANX runtime shim — the tiny bit of real C the compiled path calls into for
 * output. Compiled fresh and linked in at `anx build` time (Phase 6); malloc
 * and calloc are libc, declared but not defined here. */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

void anx_print_int(int64_t value) {
    printf("%lld\n", (long long)value);
}

void anx_print_float(double value) {
    printf("%g\n", value);
}

void anx_print_bool(int8_t value) {
    printf("%s\n", value ? "true" : "false");
}

void anx_print_str(const char *value) {
    printf("%s\n", value);
}

/* Only handles int[] — the sole array element type any P0 benchmark ever
 * prints directly (none do, in fact; this exists because the Implementation
 * Plan's Phase 5 scope names it explicitly). Not generic over element type. */
void anx_print_array(int64_t length, const int64_t *data) {
    printf("[");
    for (int64_t i = 0; i < length; i++) {
        if (i > 0) printf(", ");
        printf("%lld", (long long)data[i]);
    }
    printf("]\n");
}

/* Runtime panics — message text and exit code 2 deliberately match the
 * interpreter's RuntimeError output, so both execution paths fail
 * identically (per docs/ANX-Usage-Flow-v1.md). */

void anx_panic_oob(int64_t index, int64_t length) {
    fprintf(stderr, "runtime error: array index %lld out of bounds for length %lld\n",
            (long long)index, (long long)length);
    exit(2);
}

void anx_panic_div_zero(void) {
    fprintf(stderr, "runtime error: division by zero\n");
    exit(2);
}

void anx_panic_neg_size(int64_t size) {
    fprintf(stderr, "runtime error: array size must be non-negative, found %lld\n",
            (long long)size);
    exit(2);
}
