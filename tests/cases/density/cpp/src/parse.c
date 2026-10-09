/// This paragraph runs over two lines of a doc comment, and it is longer
/// than the limit the config sets.
int parse(void);

// A plain comment that is also far longer than the limit the config sets,
// and runs on over a second line.
int other(void);

/**
 * A block comment paragraph that runs past the limit the config sets,
 * and goes on over a second line of the block.
 */
int third(void);
