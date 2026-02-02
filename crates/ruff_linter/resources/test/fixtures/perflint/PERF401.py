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


async def f():
    items = [1, 2, 3, 4]
    result = []
    async for i in items:
        if i % 2:
            result.append(i)  # PERF401


async def f():
    items = [1, 2, 3, 4]
    result = []
    async for i in items:
        result.append(i)  # PERF401


async def f():
    items = [1, 2, 3, 4]
    result = [1, 2]
    async for i in items:
        result.append(i)  # PERF401


def f():
    result, _ = [1, 2, 3, 4], ...
    for i in range(10):
        result.append(i * 2)  # PERF401


def f():
    result = []
    if True:
        for i in range(10):  # single-line comment 1 should be protected
            # single-line comment 2 should be protected
            if i % 2:  # single-line comment 3 should be protected
                result.append(i)  # PERF401


def f():
    result = []  # comment after assignment should be protected
    for i in range(10):  # single-line comment 1 should be protected
        # single-line comment 2 should be protected
        if i % 2:  # single-line comment 3 should be protected
            result.append(i)  # PERF401


def f():
    result = []
    for i in range(10):
        """block comment stops the fix"""
        result.append(i * 2)  # Ok


def f(param):
    # PERF401
    # make sure the fix does not panic if there is no comments
    if param:
        new_layers = []
        for value in param:
            new_layers.append(value * 3)


def f():
    result = []
    var = 1
    for _ in range(10):
        result.append(var + 1)  # PERF401


def f():
    # make sure that `tmp` is not deleted
    tmp = 1; result = []  # comment should be protected
    for i in range(10):
        result.append(i + 1)  # PERF401


def f():
    # make sure that `tmp` is not deleted
    result = []; tmp = 1  # comment should be protected
    for i in range(10):
        result.append(i + 1)  # PERF401


def f():
    result = []  # comment should be protected
    for i in range(10):
        result.append(i * 2)  # PERF401


def f():
    result = []
    result.append(1)
    for i in range(10):
        result.append(i * 2)  # PERF401


def f():
    result = []
    result += [1]
    for i in range(10):
        result.append(i * 2)  # PERF401


def f():
    result = []
    for val in range(5):
        result.append(val * 2)  # Ok
    print(val)


def f():
    result = []
    for val in range(5):
        result.append(val * 2)  # PERF401
    val = 1
    print(val)


def f():
    i = [1, 2, 3]
    result = []
    for i in i:
        result.append(i + 1)  # PERF401


def f():
    result = []
    for i in range(  # Comment 1 should not be duplicated
        (
            2  # Comment 2
            + 1
        )
    ):  # Comment 3
        if i % 2:  # Comment 4
            result.append(
                (
                    i + 1,
                    # Comment 5
                    2,
                )
            )  # PERF401


def f():
    result: list[int] = []
    for i in range(10):
        result.append(i * 2)  # PERF401


def f():
    a, b = [1, 2, 3], [4, 5, 6]
    result = []
    for i in a, b:
        result.append(i[0] + i[1])  # PERF401
    return result


def f():
    values = [1, 2, 3]
    result = []
    for a in values:
        print(a)
    for a in values:
        result.append(a + 1)  # PERF401

def f():
    values = [1, 2, 3]
    def g():
        for a in values:
            result.append(a + 1)  # PERF401
    result = []

def f():
    values = [1, 2, 3]
    result = []
    for i in values:
        result.append(i + 1)  # Ok
    del i

# The fix here must parenthesize the walrus operator
# https://github.com/astral-sh/ruff/issues/15047
def f():
    items = []

    for i in range(5):
        if j := i:
            items.append(j)

def f():
    values = [1, 2, 3]
    result = list()  # this should be replaced with a comprehension
    for i in values:
        result.append(i + 1)  # PERF401

def f():
    src = [1]
    dst = []

    for i in src:
        if True if True else False:
            dst.append(i)

    for i in src:
        if lambda: 0:
            dst.append(i)

def f():
    i = "xyz"
    result = []
    for i in range(3):
        result.append(x for x in [i])

def f():
    i = "xyz"
    result = []
    for i in range(3):
        result.append((x for x in [i]))

G_INDEX = None
def f():
    global G_INDEX
    result = []
    for G_INDEX in range(3):
        result.append(G_INDEX)

def f():
    NL_INDEX = None
    def x():
        nonlocal NL_INDEX
        result = []
        for NL_INDEX in range(3):
            result.append(NL_INDEX)