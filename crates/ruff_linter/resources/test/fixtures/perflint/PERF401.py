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
            result.append(i)  # Ok
        elif i % 2:
            result.append(i)
        else:
            result.append(i)


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


class Foo:
    def append(self, x):
        pass


def f():
    items = [1, 2, 3, 4]
    result = Foo()
    for i in items:
        result.append(i)  # Ok


def f():
    items = [1, 2, 3, 4]
    result = []
    async for i in items:
        if i % 2:
            result.append(i)  # PERF401


def f():
    items = [1, 2, 3, 4]
    result = []
    async for i in items:
        result.append(i)  # PERF401


def f():
    result, _ = [1,2,3,4], ...
    for i in range(10):
        result.append(i*2)  # PERF401


def f():
    result = []
    if True:
        for i in range(10):  # single-line comment 1 should be protected
            # single-line comment 2 should be protected
            if i % 2: # single-line comment 3 should be protected 
                result.append(i) # PERF401


def f():
    result = [] # comment after assignment should be protected
    for i in range(10):  # single-line comment 1 should be protected
        # single-line comment 2 should be protected
        if i % 2: # single-line comment 3 should be protected 
            result.append(i) # PERF401


def f():
    result = []
    for i in range(10):
        """block comment stops the fix"""
        result.append(i*2)  # Ok
