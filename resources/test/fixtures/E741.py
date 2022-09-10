from contextlib import contextmanager

l = 0
I = 0
O = 0
l: int = 0

a, l = 0, 1
[a, l] = 0, 1
a, *l = 0, 1, 2
a = l = 0

o = 0
i = 0

for l in range(3):
    pass


for a, l in zip(range(3), range(3)):
    pass


def f1():
    global l
    l = 0


def f2():
    l = 0

    def f3():
        nonlocal l
        l = 1

    f3()
    return l


def f4(l):
    return l


@contextmanager
def ctx1():
    yield 0


with ctx1() as l:
    pass


@contextmanager
def ctx2():
    yield 0, 1


with ctx2() as (a, l):
    pass

try:
    pass
except ValueError as l:
    pass
