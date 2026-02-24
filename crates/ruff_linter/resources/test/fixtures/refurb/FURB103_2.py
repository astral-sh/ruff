# FURB103
with open("file.txt", "w", encoding="utf-8") as f:
    f.write("\n")
f = object()
print(f)

# See: https://github.com/astral-sh/ruff/issues/21483
with open("file.txt", "w") as f:
    f.write("\n")
print(f.encoding)
