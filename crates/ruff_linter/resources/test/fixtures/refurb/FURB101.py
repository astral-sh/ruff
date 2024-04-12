def foo():
    ...


def bar(x):
    ...


# Errors.

# FURB101
with open("file.txt") as f:
    x = f.read()

# FURB101
with open("file.txt", "rb") as f:
    x = f.read()

# FURB101
with open("file.txt", mode="rb") as f:
    x = f.read()

# FURB101
with open("file.txt", encoding="utf8") as f:
    x = f.read()

# FURB101
with open("file.txt", errors="ignore") as f:
    x = f.read()

# FURB101
with open("file.txt", mode="r") as f:  # noqa: FURB120
    x = f.read()

# FURB101
with open(foo(), "rb") as f:
    # The body of `with` is non-trivial, but the recommendation holds.
    bar("pre")
    bar(f.read())
    bar("post")
    print("Done")

# FURB101
with open("a.txt") as a, open("b.txt", "rb") as b:
    x = a.read()
    y = b.read()

# FURB101
with foo() as a, open("file.txt") as b, foo() as c:
    # We have other things in here, multiple with items, but
    # the user reads the whole file and that bit they can replace.
    bar(a)
    bar(bar(a + b.read()))
    bar(c)


# Non-errors.

# Path.read_bytes does not support any kwargs
with open("file.txt", errors="ignore", mode="rb") as f:
    x = f.read()


f2 = open("file2.txt")
with open("file.txt") as f:
    x = f2.read()

with open("file.txt") as f:
    # Path.read_text() does not support size, so ignore this
    x = f.read(100)

# mode is dynamic
with open("file.txt", foo()) as f:
    x = f.read()

# keyword mode is incorrect
with open("file.txt", mode="a+") as f:
    x = f.read()

# enables line buffering, not supported in read_text()
with open("file.txt", buffering=1) as f:
    x = f.read()

# force CRLF, not supported in read_text()
with open("file.txt", newline="\r\n") as f:
    x = f.read()

# dont mistake "newline" for "mode"
with open("file.txt", newline="b") as f:
    x = f.read()

# I guess we can possibly also report this case, but the question
# is why the user would put "r+" here in the first place.
with open("file.txt", "r+") as f:
    x = f.read()

# Even though we read the whole file, we do other things.
with open("file.txt") as f:
    x = f.read()
    f.seek(0)
    x += f.read(100)

# This shouldn't error, since it could contain unsupported arguments, like `buffering`.
with open(*filename) as f:
    x = f.read()

# This shouldn't error, since it could contain unsupported arguments, like `buffering`.
with open(**kwargs) as f:
    x = f.read()

# This shouldn't error, since it could contain unsupported arguments, like `buffering`.
with open("file.txt", **kwargs) as f:
    x = f.read()

# This shouldn't error, since it could contain unsupported arguments, like `buffering`.
with open("file.txt", mode="r", **kwargs) as f:
    x = f.read()

# This could error (but doesn't), since it can't contain unsupported arguments, like
# `buffering`.
with open(*filename, mode="r") as f:
    x = f.read()

# This could error (but doesn't), since it can't contain unsupported arguments, like
# `buffering`.
with open(*filename, file="file.txt", mode="r") as f:
    x = f.read()
