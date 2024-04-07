# Errors


def a():
    l = []
    l = reversed(l)


def b():
    l = []
    l = list(reversed(l))


def c():
    l = []
    l = l[::-1]


# False negative
def c2():
    class Wrapper:
        l: list[int]

    w = Wrapper()
    w.l = list(reversed(w.l))
    w.l = w.l[::-1]
    w.l = reversed(w.l)


# OK


def d():
    l = []
    _ = reversed(l)


def e():
    l = []
    l = l[::-2]
    l = l[1:]
    l = l[1::-1]
    l = l[:1:-1]


def f():
    d = {}

    # Don't warn: `d` is a dictionary, which doesn't have a `reverse` method.
    d = reversed(d)


def g():
    l = "abc"[::-1]


def h():
    l = reversed([1, 2, 3])


def i():
    l = list(reversed([1, 2, 3]))
