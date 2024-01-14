A[*b]
A[*b] = 1
A
del A[*b]
A
A[*b, *b]
A[*b, *b] = 1
A
del A[*b, *b]
A
A[b, *b]
A[b, *b] = 1
A
del A[b, *b]
A
A[*b, b]
A[*b, b] = 1
A
del A[*b, b]
A
A[b, b, *b]
A[b, b, *b] = 1
A
del A[b, b, *b]
A
A[*b, b, b]
A[*b, b, b] = 1
A
del A[*b, b, b]
A
A[b, *b, b]
A[b, *b, b] = 1
A
del A[b, *b, b]
A
A[b, b, *b, b]
A[b, b, *b, b] = 1
A
del A[b, b, *b, b]
A
A[b, *b, b, b]
A[b, *b, b, b] = 1
A
del A[b, *b, b, b]
A
A[A[b, *b, b]]
A[A[b, *b, b]] = 1
A
del A[A[b, *b, b]]
A
A[*A[b, *b, b]]
A[*A[b, *b, b]] = 1
A
del A[*A[b, *b, b]]
A
A[b, ...]
A[b, ...] = 1
A
del A[b, ...]
A
A[*A[b, ...]]
A[*A[b, ...]] = 1
A
del A[*A[b, ...]]
A
l = [1, 2, 3]
A[*l]
A[*l] = 1
A
del A[*l]
A
A[*l, 4]
A[*l, 4] = 1
A
del A[*l, 4]
A
A[0, *l]
A[0, *l] = 1
A
del A[0, *l]
A
A[1:2, *l]
A[1:2, *l] = 1
A
del A[1:2, *l]
A
repr(A[1:2, *l]) == repr(A[1:2, 1, 2, 3])
t = (1, 2, 3)
A[*t]
A[*t] = 1
A
del A[*t]
A
A[*t, 4]
A[*t, 4] = 1
A
del A[*t, 4]
A
A[0, *t]
A[0, *t] = 1
A
del A[0, *t]
A
A[1:2, *t]
A[1:2, *t] = 1
A
del A[1:2, *t]
A
repr(A[1:2, *t]) == repr(A[1:2, 1, 2, 3])


def returns_list():
    return [1, 2, 3]


A[returns_list()]
A[returns_list()] = 1
A
del A[returns_list()]
A
A[returns_list(), 4]
A[returns_list(), 4] = 1
A
del A[returns_list(), 4]
A
A[*returns_list()]
A[*returns_list()] = 1
A
del A[*returns_list()]
A
A[*returns_list(), 4]
A[*returns_list(), 4] = 1
A
del A[*returns_list(), 4]
A
A[0, *returns_list()]
A[0, *returns_list()] = 1
A
del A[0, *returns_list()]
A
A[*returns_list(), *returns_list()]
A[*returns_list(), *returns_list()] = 1
A
del A[*returns_list(), *returns_list()]
A
A[1:2, *b]
A[*b, 1:2]
A[1:2, *b, 1:2]
A[*b, 1:2, *b]
A[1:, *b]
A[*b, 1:]
A[1:, *b, 1:]
A[*b, 1:, *b]
A[:1, *b]
A[*b, :1]
A[:1, *b, :1]
A[*b, :1, *b]
A[:, *b]
A[*b, :]
A[:, *b, :]
A[*b, :, *b]
A[a * b()]
A[a * b(), *c, *d(), e * f(g * h)]
A[a * b(), :]
A[a * b(), *c, *d(), e * f(g * h) :]
A[[b] * len(c), :]


def f1(*args: *b):
    pass


f1.__annotations__


def f2(*args: *b, arg1):
    pass


f2.__annotations__


def f3(*args: *b, arg1: int):
    pass


f3.__annotations__


def f4(*args: *b, arg1: int = 2):
    pass


f4.__annotations__
