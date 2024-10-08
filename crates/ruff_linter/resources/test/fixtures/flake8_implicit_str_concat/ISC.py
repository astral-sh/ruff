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
