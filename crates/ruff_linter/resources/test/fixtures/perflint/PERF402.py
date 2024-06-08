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


def f():
    items = [1, 2, 3, 4]
    result = {}
    for i in items:
        result[i].append(i * i)  # OK


class Foo:
    def append(self, x):
        pass


def f():
    items = [1, 2, 3, 4]
    result = Foo()
    for i in items:
        result.append(i)  # OK


def f():
    import sys

    for path in ("foo", "bar"):
        sys.path.append(path)  # OK


def f():
    items = [1, 2, 3, 4]
    result = []
    async for i in items:
        result.append(i)  # PERF402
