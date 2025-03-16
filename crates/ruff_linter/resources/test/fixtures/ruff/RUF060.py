d = {"a": 1, "b": 2}
to_list = {"a": [1, 2]}
to_set = {"a": {1, 2}}
s = "s"
def get_s() -> str:
    return "s"
def expensive() -> int:
    return 3

# Errors
if s not in d:
    d[s] = 3

if "c" not in d:
    d["c"] = 3

d["c"] = d.get("c", 3)

if (x := get_s()) not in d:
    d[x] = 3

if "c" in to_list:
    to_list["c"].append(3)
else:
    to_list["c"] = [3]

if "c" not in to_list:
    to_list["c"] = [3]
else:
    to_list["c"].append(3)

if "c" in to_list:
    to_list["c"].append(expensive())
else:
    to_list["c"] = [expensive()]

if "c" in to_set:
    to_set["c"].add(3)
else:
    to_set["c"] = {3}

def foo(**kwargs):
    if "option" not in kwargs:
        kwargs["option"] = 3


# Errors (unsafe).
if "c" not in d:
    # important comment
    d["c"] = 3

if "c" not in d:
    d["c"] = 1 + 1

if "c" not in d:
    d["c"] = expensive()

if get_s() not in d:
    d[get_s()] = 3


# Ok
l = [1, 2, 3]
if 1 in l:
    l[1] = 3

if "c" in d:
    d["c"] = 3

if "c" not in d:
    print("c is missing")
    d["c"] = 3

if "c" not in d:
    d["d"] = 3

d["c"] = d.get("d", 3)

if "c" in to_list:
    to_list["c"].append(3)
else:
    to_list["d"] = [3]

if "c" in to_list:
    to_list["c"].append(3)
else:
    to_list["c"] = [4]
