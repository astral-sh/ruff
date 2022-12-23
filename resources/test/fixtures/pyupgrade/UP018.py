# These remain unchanged
str(1)
str(*a)
str("foo", *a)
str(**k)
str("foo", **k)
str("foo", encoding="UTF-8")
str("foo"
    "bar")
bytes("foo", encoding="UTF-8")
bytes(*a)
bytes("foo", *a)
bytes("foo", **a)
bytes(b"foo"
      b"bar")

# These become string or byte literals
str()
str("foo")
str("""
foo""")
bytes()
bytes(b"foo")
bytes(b"""
foo""")
