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
            result[idx] = name  # Ok (false negative: edge case where `else` is same as `if`
        else:
            result[idx] = name


def foo():
    result = {}
    fruit = ["apple", "pear", "orange"]
    for idx, name in enumerate(fruit):
        if idx % 2:
            result[idx] = name  # OK (false negative due to placement of `result = {}`)


def foo():
    fruit = ["apple", "pear", "orange"]
    result = []
    for idx, name in enumerate(fruit):
        if idx % 2:
            result[idx] = name  # OK because result is not a dictionary
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
            result[idx] = name  # Ok
