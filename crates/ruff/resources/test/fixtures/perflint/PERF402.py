def f():
    items = [1, 2, 3, 4]
    result = []
    for i in items:
        result.append(i)  # PERF402


def f():
    items = [1, 2, 3, 4]
    result = []
    for i in items:
        result.insert(0, i)  # PERF402


def f():
    items = [1, 2, 3, 4]
    result = []
    for i in items:
        result.append(i * i)  # OK
