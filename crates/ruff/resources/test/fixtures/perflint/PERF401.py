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


def f():
    fibonacci = [0, 1]
    for i in range(20):
        fibonacci.append(sum(fibonacci[-2:]))  # OK
    print(fibonacci)


def f():
    foo = object()
    foo.fibonacci = [0, 1]
    for i in range(20):
        foo.fibonacci.append(sum(foo.fibonacci[-2:]))  # OK
    print(foo.fibonacci)
