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

# RUF019 (f-string fix safety)
class C:
    def __str__(self):
        return "k"


c = C()

# Unsafe: interpolation may call user-defined `__str__`.
if f"{c}" in d and d[f"{c}"]:
    pass

# Safe: literal-only f-string cases.
if f"k" in d and d[f"k"]:
    pass

if f"{1}" in d and d[f"{1}"]:
    pass

if f"{-1}" in d and d[f"{-1}"]:
    pass

if f"{(1, 2)}" in d and d[f"{(1, 2)}"]:
    pass


if (
        "key" in d
        and  # text
        d ["key"]
):
    ...
