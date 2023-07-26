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

_ = (
  a + f"abc" +
  "def"
)

_ = (
  f"abc" +
  "def" + a
)
