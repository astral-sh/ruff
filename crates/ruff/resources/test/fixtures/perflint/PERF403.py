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
            result[idx] = name  # Ok because if/elif/else not replaceable by dict comprehension
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


def foo():
    result = {1: "banana"}
    fruit = ["apple", "pear", "orange"]
    for idx, name in enumerate(fruit):
        if idx % 2:
            result[idx] = name  # Ok because dict was not empty before loop
        else:
            result[idx] = name


def foo():
    result = []
    fruit = ["apple", "pear", "orange"]
    for idx, name in enumerate(fruit):
        if idx % 2:
            result[idx] = name  # Ok because result is not a dictionary
        else:
            result[idx] = name

