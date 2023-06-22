def foo():
    items = [1, 2, 3, 4]
    result = []
    for i in items:
        if i % 2:
            result.append(i)  # PERF401


def foo():
    items = [1,2,3,4]
    result = []
    for i in items:
        if i % 2:
            result.append(i)  # PERF401
        elif i % 2:
            result.append(i)  # PERF401
        else:
            result.append(i)  # PERF401


