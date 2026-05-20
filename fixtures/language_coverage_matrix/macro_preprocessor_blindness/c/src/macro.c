#define MAKE_FN(name) int name(void) { return 1; }
MAKE_FN(generated)
int caller(void) { return generated(); }
