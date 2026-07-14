/* ANX runtime shim — the tiny bit of real C the compiled path calls into for
 * output. Compiled fresh and linked in at `anx build` time (Phase 6); malloc
 * and calloc are libc, declared but not defined here. */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Mirrors codegen's `{ i64 length, ptr data }` struct exactly (see
 * docs/P2/ANX-P2-Strings-Plan-v1.md Step 3) — struct-by-value across this
 * boundary relies on the same ABI lowering already proven for array
 * parameters passed between ANX functions. `data` is always null-terminated
 * so `anx_print_str` can keep treating it as a plain C string. */
typedef struct {
    int64_t length;
    const char *data;
} AnxStr;

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

/* String ops (Step 3 of the P2 Strings plan). Bounds-checking for
 * charAt/substring happens in codegen (an inline guard calling
 * anx_panic_str_oob, mirroring the array index check) *before* these are
 * called — so the indices/lengths here are already known valid and never
 * re-validated. Every new buffer is malloc'd and null-terminated, matching
 * the "leak on exit, never free" memory model used everywhere else. */

AnxStr anx_str_concat(AnxStr a, AnxStr b) {
    int64_t new_len = a.length + b.length;
    char *buf = malloc(new_len + 1);
    memcpy(buf, a.data, a.length);
    memcpy(buf + a.length, b.data, b.length);
    buf[new_len] = '\0';
    AnxStr result = { new_len, buf };
    return result;
}

AnxStr anx_str_char_at(AnxStr s, int64_t i) {
    char *buf = malloc(2);
    buf[0] = s.data[i];
    buf[1] = '\0';
    AnxStr result = { 1, buf };
    return result;
}

AnxStr anx_str_substring(AnxStr s, int64_t start, int64_t end) {
    int64_t new_len = end - start;
    char *buf = malloc(new_len + 1);
    memcpy(buf, s.data + start, new_len);
    buf[new_len] = '\0';
    AnxStr result = { new_len, buf };
    return result;
}

int8_t anx_str_equals(AnxStr a, AnxStr b) {
    if (a.length != b.length) return 0;
    return memcmp(a.data, b.data, a.length) == 0;
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

void anx_panic_str_oob(int64_t index, int64_t length) {
    fprintf(stderr, "runtime error: string index %lld out of bounds for length %lld\n",
            (long long)index, (long long)length);
    exit(2);
}
