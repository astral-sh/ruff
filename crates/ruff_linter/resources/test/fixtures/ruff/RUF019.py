d = {}
# RUF019
if "k" in d and d["k"]:
    pass

k = "k"
if k in d and d[k]:
    pass

if (k) in d and d[k]:
    pass

if k in d and d[(k)]:
    pass

not ("key" in dct and dct["key"])

bool("key" in dct and dct["key"])

# OK
v = "k" in d and d["k"]

if f() in d and d[f()]:
    pass


if (
        "key" in d
        and  # text
        d ["key"]
):
    ...

# https://github.com/astral-sh/ruff/issues/12953
# F-string with non-literal interpolation — unsafe fix (may invoke __str__)
class Formatter:
    def __str__(self):
        print("side effect!")
        return "key"

c = Formatter()
if f"{c}" in d and d[f"{c}"]:
    pass

# F-string with only literal interpolation — safe fix
if f"{1}" in d and d[f"{1}"]:
    pass

# Plain f-string without interpolation — safe fix
if f"key" in d and d[f"key"]:
    pass

# Walrus operator is a side effect — should not emit
if (k := "key") in d and d[(k := "key")]:
    pass
