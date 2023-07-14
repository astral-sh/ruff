def f():
    items = [1, 2, 3, 4]
    result = []
    for i in items:
        if i % 2:
            result.append(i)  # PERF401


def f():
    items = [1, 2, 3, 4]
    result = []
    for i in items:
        result.append(i * i)  # PERF401


def f():
    items = [1, 2, 3, 4]
    result = []
    for i in items:
        if i % 2:
            result.append(i)  # PERF401
        elif i % 2:
            result.append(i)  # PERF401
        else:
            result.append(i)  # PERF401


def f():
    items = [1, 2, 3, 4]
    result = []
    for i in items:
        result.append(i)  # OK


def f():
    items = [1, 2, 3, 4]
    result = {}
    for i in items:
        result[i].append(i)  # OK


def f():
    items = [1, 2, 3, 4]
    result = []
    for i in items:
        if i not in result:
            result.append(i)  # OK
