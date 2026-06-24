_ = "a" "b" "c"

_ = "abc" + "def"

_ = "abc" \
    "def"

_ = (
  "abc" +
  "def"
)

_ = (
  f"abc" +
  "def"
)

_ = (
  b"abc" +
  b"def"
)

_ = (
  "abc"
  "def"
)

_ = (
  f"abc"
  "def"
)

_ = (
  b"abc"
  b"def"
)

_ = """a""" """b"""

_ = """a
b""" """c
d"""

_ = f"""a""" f"""b"""

_ = f"a" "b"

_ = """a""" "b"

_ = 'a' "b"

_ = rf"a" rf"b"

# Single-line explicit concatenation should be ignored.
_ = "abc" + "def" + "ghi"
_ = foo + "abc" + "def"
_ = "abc" + foo + "def"
_ = "abc" + "def" + foo
_ = foo + bar + "abc"
_ = "abc" + foo + bar
_ = foo + "abc" + bar

# Multiple strings nested inside a f-string
_ = f"a {'b' 'c' 'd'} e"
_ = f"""abc {"def" "ghi"} jkl"""
_ = f"""abc {
    "def"
    "ghi"
} jkl"""

# Nested f-strings
_ = "a" f"b {f"c" f"d"} e" "f"
_ = f"b {f"c" f"d {f"e" f"f"} g"} h"
_ = f"b {f"abc" \
    f"def"} g"

# Explicitly concatenated nested f-strings
_ = f"a {f"first"
    + f"second"} d"
_ = f"a {f"first {f"middle"}"
    + f"second"} d"

# See https://github.com/astral-sh/ruff/issues/12936
_ = "\12""0" # fix should be "\0120"
_ = "\\12""0" # fix should be "\\120"
_ = "\\\12""0" # fix should be "\\\0120"
_ = "\12 0""0" # fix should be "\12 00"
_ = r"\12"r"0" # fix should be r"\120"
_ = "\12 and more""0" # fix should be "\12 and more0"
_ = "\8""0" # fix should be "\80"
_ = "\12""8" # fix should be "\128"
_ = "\12""foo" # fix should be "\12foo"
_ = "\12" ""  # fix should be "\12"


# Mixed literal + non-literal scenarios
_ = (
    "start" +
    variable +
    "end"
)

_ = (
    f"format" +
    func_call() +
    "literal"
)

_ = (
    rf"raw_f{x}" +
    r"raw_normal"
)


# Different prefix combinations
_ = (
    u"unicode" +
    r"raw"
)

_ = (
    rb"raw_bytes" +
    b"normal_bytes"
)

_ = (
    b"bytes" +
    b"with_bytes"
)

# Repeated concatenation

_ = ("a" +
    "b" +
    "c" +
    "d" + "e"
)

_ = ("a"
    + "b"
    + "c"
    + "d"
    + "e"
)

_ = (
    "start" +
    variable + # comment
    "end"
)

_ = (
    "start" +
    variable
		# leading comment
    + "end"
)

_ = (
    "first"
    +    "second"  # extra spaces around +
)

_ = (
    "first"    +  # trailing spaces before +
    "second"
)

_ = ((
    "deep" +
    "nesting"
))

_ = (
    "contains + plus" +
    "another string"
)

_ = (
    "start"
		# leading comment
    + "end"
)

_ = (
    "start" +
		# leading comment
    "end"
)

# https://github.com/astral-sh/ruff/issues/20310
# ISC001
t"The quick " t"brown fox."

# ISC002
t"The quick brown fox jumps over the lazy "\
t"dog."

# ISC003
(
    t"The quick brown fox jumps over the lazy "
    + t"dog"
)

# nested examples with both t and f-strings
_ = "a" f"b {t"c" t"d"} e" "f"
_ = t"b {f"c" f"d {t"e" t"f"} g"} h"
_ = f"b {t"abc" \
    t"def"} g"


# Explicit concatenation with either operand being
# a string literal that wraps across multiple lines (in parentheses)
# reports diagnostic - no autofix.
# See https://github.com/astral-sh/ruff/issues/19757
_ = "abc" + (
    "def"
    "ghi"
)

_ = (
    "abc"
    "def"
) + "ghi"
