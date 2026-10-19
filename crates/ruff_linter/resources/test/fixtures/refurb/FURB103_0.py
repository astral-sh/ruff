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

# See: https://github.com/astral-sh/ruff/issues/20785
import json

data = {"price": 100}

with open("test.json", "wb") as f:
    f.write(json.dumps(data, indent=4).encode("utf-8"))

# See: https://github.com/astral-sh/ruff/issues/21381
with open("tmp_path/pyproject.toml", "w") as f:
    f.write(dedent(
        """
        [project]
        other = 1.234
        """,
    ))


# `open` accepts a file descriptor, but `Path` does not
with open(3, "w") as f:
    f.write("test")


# `open` truncates the file before `write` checks the type of the value, while `write_text` and
# `write_bytes` check it before they open the file. The fix is only safe when the value is known
# to have the type that the mode implies.
def known_types(data: bytes, text: str):
    # FURB103 - safe, `text` is a string and the file is opened in text mode
    with open("file.txt", "w") as f:
        f.write(text)

    # FURB103 - safe, `data` is bytes and the file is opened in binary mode
    with open("file.txt", "wb") as f:
        f.write(data)

    # FURB103 - unsafe, `text` is a string but the file is opened in binary mode
    with open("file.txt", "wb") as f:
        f.write(text)

    # FURB103 - unsafe, `data` is bytes but the file is opened in text mode
    with open("file.txt", "w") as f:
        f.write(data)

    # FURB103 - unsafe, an f-string can raise before the file is opened
    with open("file.txt", "w") as f:
        f.write(f"{text}")

    # FURB103 - unsafe, a call is not resolved to a type
    with open("file.txt", "w") as f:
        f.write(str(data))


class Attribute:
    text: str

    def write(self):
        # FURB103 - unsafe, `self.text` is not resolved to a type
        with open("file.txt", "w") as f:
            f.write(self.text)


def unknown_types(value):
    # FURB103 - unsafe, the type of `value` is unknown
    with open("file.txt", "w") as f:
        f.write(value)

    # FURB103 - unsafe, the type of `value` is unknown
    with open("file.txt", "wb") as f:
        f.write(value)

    # FURB103 - unsafe, `number` is an `int`
    number = 1
    with open("file.txt", "w") as f:
        f.write(number)
