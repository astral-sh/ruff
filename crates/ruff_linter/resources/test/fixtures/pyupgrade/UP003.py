type("")
type(b"")
type(0)
type(0.0)
type(0j)

# OK
type(arg)(" ")

# OK
y = x.dtype.type(0.0)

# Regression test for: https://github.com/astral-sh/ruff/issues/7455#issuecomment-1722459841
assert isinstance(fullname, type("")is not True)
