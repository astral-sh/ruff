# The lexer doesn't emit a string token if it's unterminated
"a" "b
"a" "b" "c
"a" """b
c""" "d

# For f-strings, the `FStringRanges` won't contain the range for
# unterminated f-strings.
f"a" f"b
f"a" f"b" f"c
f"a" f"""b
c""" f"d {e

(
    "a"
    "b
    "c"
    "d"
)


# Triple-quoted strings, if unterminated, consume everything that comes after
# the opening quote. So, no test code should raise the violation after this.
(
    """abc"""
    f"""def
    "g" "h"
    "i" "j"
)
