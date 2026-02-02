def foo():
    fruit = ["apple", "pear", "orange"]
    result = {}
    for idx, name in enumerate(fruit):
        result[idx] = name  # PERF403


def foo():
    fruit = ["apple", "pear", "orange"]
    result = {}
    for idx, name in enumerate(fruit):
        if idx % 2:
            result[idx] = name  # PERF403


def foo():
    fruit = ["apple", "pear", "orange"]
    result = {}
    for idx, name in enumerate(fruit):
        if idx % 2:
            result[idx] = (
                name  # Ok (false negative: edge case where `else` is same as `if`)
            )
        else:
            result[idx] = name


def foo():
    result = {}
    fruit = ["apple", "pear", "orange"]
    for idx, name in enumerate(fruit):
        if idx % 2:
            result[idx] = name  # PERF403


def foo():
    fruit = ["apple", "pear", "orange"]
    result = []
    for idx, name in enumerate(fruit):
        if idx % 2:
            result[idx] = name  # OK (result is not a dictionary)
        else:
            result[idx] = name


def foo():
    fruit = ["apple", "pear", "orange"]
    result = {}
    for idx, name in enumerate(fruit):
        if idx % 2:
            result[idx] = name  # OK (if/elif/else isn't replaceable)
        elif idx % 3:
            result[idx] = name
        else:
            result[idx] = name


def foo():
    result = {1: "banana"}
    fruit = ["apple", "pear", "orange"]
    for idx, name in enumerate(fruit):
        if idx % 2:
            result[idx] = name  # PERF403


def foo():
    fruit = ["apple", "pear", "orange"]
    result = {}
    for idx, name in enumerate(fruit):
        if idx in result:
            result[idx] = name  # PERF403


def foo():
    fruit = ["apple", "pear", "orange"]
    result = {}
    for name in fruit:
        result[name] = name  # PERF403


def foo():
    fruit = ["apple", "pear", "orange"]
    result = {}
    for idx, name in enumerate(fruit):
        result[name] = idx  # PERF403


def foo():
    from builtins import dict as SneakyDict

    fruit = ["apple", "pear", "orange"]
    result = SneakyDict()
    for idx, name in enumerate(fruit):
        result[name] = idx  # PERF403


def foo():
    fruit = ["apple", "pear", "orange"]
    result: dict[str, int] = {
        # comment 1
    }
    for idx, name in enumerate(
        fruit  # comment 2
    ):
        # comment 3
        result[
            name  # comment 4
        ] = idx  # PERF403


def foo():
    fruit = ["apple", "pear", "orange"]
    a = 1; result = {}; b = 2
    for idx, name in enumerate(fruit):
        result[name] = idx  # PERF403


def foo():
    fruit = ["apple", "pear", "orange"]
    result = {"kiwi": 3}
    for idx, name in enumerate(fruit):
        result[name] = idx  # PERF403


def foo():
    fruit = ["apple", "pear", "orange"]
    (_, result) = (None, {"kiwi": 3})
    for idx, name in enumerate(fruit):
        result[name] = idx  # PERF403


def foo():
    fruit = ["apple", "pear", "orange"]
    result = {}
    print(len(result))
    for idx, name in enumerate(fruit):
        result[name] = idx  # PERF403


def foo():
    fruit = ["apple", "pear", "orange"]
    result = {}
    for idx, name in enumerate(fruit):
        if last_idx := idx % 3:
            result[name] = idx  # PERF403


def foo():
    fruit = ["apple", "pear", "orange"]
    indices = [0, 1, 2]
    result = {}
    for idx, name in indices, fruit:
        result[name] = idx  # PERF403


def foo():
    src = (("x", 1),)
    dst = {}

    for k, v in src:
        if True if True else False:
            dst[k] = v

    for k, v in src:
        if lambda: 0:
            dst[k] = v

# https://github.com/astral-sh/ruff/issues/18859
def foo():
    v = {}
    for o,(x,)in():
        v[x,]=o


# https://github.com/astral-sh/ruff/issues/19005
def issue_19005_1():
    c = {}
    a = object()
    for a.b in ():
        c[a.b] = a.b


def issue_19005_2():
    a = object()
    c = {}
    for a.k, a.v in ():
        c[a.k] = a.v


def issue_19005_3():
    a = [None, None]
    c = {}
    for a[0], a[1] in ():
        c[a[0]] = a[1]


def issue_19153_1():
    v = {}
    for o, (x,) in ["ox"]:
        v[x,] = o
    return v


def issue_19153_2():
    v = {}
    for (o, p), x in [("op", "x")]:
        v[x] = o, p
    return v


def issue_19153_3():
    v = {}
    for o, (x,) in ["ox"]: 
        v[(x,)] = o 
    return v