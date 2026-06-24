# Tests for Python 3.13+ where `pathlib.Path.read_text` supports `newline`.

# FURB101 (newline is supported in read_text on Python 3.13+)
with open("file.txt", newline="\r\n") as f:
    x = f.read()

# FURB101 (newline with encoding)
with open("file.txt", encoding="utf-8", newline="") as f:
    x = f.read()

# FURB101 (newline=None is also valid)
with open("file.txt", newline=None) as f:
    x = f.read()
