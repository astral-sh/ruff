from pathlib import Path

with Path("file.txt").open("w") as f:
    f.write("test")

with Path("file.txt").open("wb") as f:
    f.write(b"test")

with Path("file.txt").open(mode="w") as f:
    f.write("test")

with Path("file.txt").open("w", encoding="utf8") as f:
    f.write("test")

with Path("file.txt").open("w", errors="ignore") as f:
    f.write("test")

with Path(foo()).open("w") as f:
    f.write("test")

p = Path("file.txt")
with p.open("w") as f:
    f.write("test")

with Path("foo", "bar", "baz").open("w") as f:
    f.write("test")