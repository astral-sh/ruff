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

# RUF019 (f-string side-effect matrix)
class C:
    def __str__(self):
        return "k"


c = C()

# side_effect -> Yes: definite side effect in interpolation; rule should not trigger.
if f"{f()}" in d and d[f"{f()}"]:
    pass

# side_effect -> Maybe: formatting may call user code; unsafe fix.
if f"{c}" in d and d[f"{c}"]:
    pass

# side_effect -> No: literal-only interpolation; safe fix.
if f"{1}" in d and d[f"{1}"]:
    pass


if (
        "key" in d
        and  # text
        d ["key"]
):
    ...
