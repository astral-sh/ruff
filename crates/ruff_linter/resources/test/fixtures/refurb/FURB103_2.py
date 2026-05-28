# FURB103
# should trigger
with open("file.txt", "w", encoding="utf-8") as f:
    f.write("\n")
f = object()
print(f)

# See: https://github.com/astral-sh/ruff/issues/21483
with open("file.txt", "w") as f:
    f.write("\n")
print(f.encoding)


def _():
    # should trigger
    with open("file.txt", "w") as f:
        f.write("\n")
    return (f.name for _ in [0])


def _set():
    # should trigger
    with open("file.txt", "w") as f:
        f.write("\n")
    g = {f.name for _ in [0]}
    return g
