int a;  // NOLINT(check) -- we delve into the tree
int b;  // NOLINT
// A NOLINT here is only a word of prose.
int f(int x) {
    switch (x) {
    case 1:
        x++;
        /* fallthrough */
    case 2:
        return x;
    }
    return 0;
}
