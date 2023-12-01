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
