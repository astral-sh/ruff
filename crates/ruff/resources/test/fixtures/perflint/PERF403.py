def foo():
    result = {}
    fruit = ["apple", "pear", "orange"]
    for idx, name in enumerate(fruit):
        result[idx] = name  # PERF403


def foo():
    result = {}
    fruit = ["apple", "pear", "orange"]
    for idx, name in enumerate(fruit):
        if idx % 2:
            result[idx] = name  # PERF403


def foo():
    result = {}
    fruit = ["apple", "pear", "orange"]
    for idx, name in enumerate(fruit):
        if idx % 2:
            result[idx] = name  # PERF403
        elif idx % 3:
            result[idx] = name
        else:
            result[idx] = name


def foo():
    result = {}
    fruit = ["apple", "pear", "orange"]
    for idx, name in enumerate(fruit):
        if idx % 2:
            result[idx] = name  # PERF403
        else:
            result[idx] = name
