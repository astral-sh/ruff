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

# OK
v = "k" in d and d["k"]

if f() in d and d[f()]:
    pass
