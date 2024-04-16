def foo():
    ...


def bar(x):
    ...


# Errors.

# FURB103
with open("file.txt", "w") as f:
    f.write("test")

# FURB103
with open("file.txt", "wb") as f:
    f.write(foobar)

# FURB103
with open("file.txt", mode="wb") as f:
    f.write(b"abc")

# FURB103
with open("file.txt", "w", encoding="utf8") as f:
    f.write(foobar)

# FURB103
with open("file.txt", "w", errors="ignore") as f:
    f.write(foobar)

# FURB103
with open("file.txt", mode="w") as f:
    f.write(foobar)

# FURB103
with open(foo(), "wb") as f:
    # The body of `with` is non-trivial, but the recommendation holds.
    bar("pre")
    f.write(bar())
    bar("post")
    print("Done")

# FURB103
with open("a.txt", "w") as a, open("b.txt", "wb") as b:
    a.write(x)
    b.write(y)

# FURB103
with foo() as a, open("file.txt", "w") as b, foo() as c:
    # We have other things in here, multiple with items, but the user
    # writes a single time to file and that bit they can replace.
    bar(a)
    b.write(bar(bar(a + x)))
    bar(c)


# FURB103
with open("file.txt", "w", newline="\r\n") as f:
    f.write(foobar)


import builtins


# FURB103
with builtins.open("file.txt", "w", newline="\r\n") as f:
    f.write(foobar)


from builtins import open as o


# FURB103
with o("file.txt", "w", newline="\r\n") as f:
    f.write(foobar)

# Non-errors.

with open("file.txt", errors="ignore", mode="wb") as f:
    # Path.write_bytes() does not support errors
    f.write(foobar)

f2 = open("file2.txt", "w")
with open("file.txt", "w") as f:
    f2.write(x)

# mode is dynamic
with open("file.txt", foo()) as f:
    f.write(x)

# keyword mode is incorrect
with open("file.txt", mode="a+") as f:
    f.write(x)

# enables line buffering, not supported in write_text()
with open("file.txt", buffering=1) as f:
    f.write(x)

# dont mistake "newline" for "mode"
with open("file.txt", newline="wb") as f:
    f.write(x)

# I guess we can possibly also report this case, but the question
# is why the user would put "w+" here in the first place.
with open("file.txt", "w+") as f:
    f.write(x)

# Even though we write the whole file, we do other things.
with open("file.txt", "w") as f:
    f.write(x)
    f.seek(0)
    x += f.read(100)

# This shouldn't error, since it could contain unsupported arguments, like `buffering`.
with open(*filename, mode="w") as f:
    f.write(x)

# This shouldn't error, since it could contain unsupported arguments, like `buffering`.
with open(**kwargs) as f:
    f.write(x)

# This shouldn't error, since it could contain unsupported arguments, like `buffering`.
with open("file.txt", **kwargs) as f:
    f.write(x)

# This shouldn't error, since it could contain unsupported arguments, like `buffering`.
with open("file.txt", mode="w", **kwargs) as f:
    f.write(x)

# This could error (but doesn't), since it can't contain unsupported arguments, like
# `buffering`.
with open(*filename, mode="w") as f:
    f.write(x)

# This could error (but doesn't), since it can't contain unsupported arguments, like
# `buffering`.
with open(*filename, file="file.txt", mode="w") as f:
    f.write(x)

# Loops imply multiple writes
with open("file.txt", "w") as f:
    while x < 0:
        f.write(foobar)

with open("file.txt", "w") as f:
    for line in text:
        f.write(line)
