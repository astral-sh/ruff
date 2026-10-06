# See: https://github.com/astral-sh/ruff/issues/26920

# Safe fix: the argument is known to match the mode.
def text_param(x: str):
    with open("file.txt", "w") as f:
        f.write(x)


def bytes_param(x: bytes):
    with open("file.txt", "wb") as f:
        f.write(x)


def fstring(x):
    with open("file.txt", "w") as f:
        f.write(f"{x}")


def text_assignment():
    x = "text"
    with open("file.txt", "w") as f:
        f.write(x)


# Unsafe fix: the argument does not match the mode, so `open` would truncate the
# file before `write` raises, but `write_text`/`write_bytes` raise first.
with open("file.txt", "w") as f:
    f.write(b"\103")

with open("file.txt", "wb") as f:
    f.write("text")


def text_param_bytes_mode(x: str):
    with open("file.txt", "wb") as f:
        f.write(x)


def bytes_param_text_mode(x: bytes):
    with open("file.txt", "w") as f:
        f.write(x)


# Unsafe fix: the type of the argument is unknown.
def untyped(x):
    with open("file.txt", "w") as f:
        f.write(x)


def call_result():
    with open("file.txt", "w") as f:
        f.write(str(1))
