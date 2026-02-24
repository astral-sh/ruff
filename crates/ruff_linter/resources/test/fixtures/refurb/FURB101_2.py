# FURB101
with open("file.txt", encoding="utf-8") as f:
    _ = f.read()
f = object()
print(f)

# See: https://github.com/astral-sh/ruff/issues/21483
with open("file.txt", encoding="utf-8") as f:
    _ = f.read()
print(f.mode)
