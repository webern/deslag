// Raw strings, digit separators and user-defined literals.
#include <string>

const char *a = R"(a "quoted" // not a comment)";
const char *b = u8R"x(a /* nor this */)x";
long n = 1'000'000; /* a comment */
auto s = "text"_s;
char c = 'c';
