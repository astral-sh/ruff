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

# FURB101 (newline is supported in read_text on Python 3.13+)
with open("file.txt", newline="\r\n") as f:
    x = f.read()

# FURB101 (dont mistake "newline" for "mode")
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

# FURB101
with open("file.txt", encoding="utf-8") as f:
    contents: str = f.read()

# FURB101 but no fix because it would remove the assignment to `x`
with open("file.txt", encoding="utf-8") as f:
    contents, x = f.read(), 2

# FURB101 but no fix because it would remove the `process_contents` call
with open("file.txt", encoding="utf-8") as f:
    contents = process_contents(f.read())

with open("file1.txt", encoding="utf-8") as f:
    contents: str = process_contents(f.read())



# See: https://github.com/astral-sh/ruff/issues/26922
# `open` accepts a file descriptor, but `Path` does not. `PTH123` already skips
# these via `is_file_descriptor`.

# No error: integer literal.
with open(3) as f:
    x = f.read()

fd: int = 3

# No error: name annotated as `int`.
with open(fd) as f:
    x = f.read()


class FileDescriptorHolder:
    fd: int


# No error: class attribute annotated as `int`.
with open(FileDescriptorHolder.fd) as f:
    x = f.read()

# FURB101: a `str` filename in the same position is still flagged.
with open("descriptor_control.txt") as f:
    x = f.read()

# FURB101: `os.open` returns a file descriptor, but proving that requires type
# inference, so this case is still flagged.
import os

os_fd = os.open("file.txt", os.O_RDONLY)
with open(os_fd) as f:
    x = f.read()
