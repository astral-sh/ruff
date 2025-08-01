# Violation cases: RUF025


def func():
    numbers = [1, 2, 3]
    {n: None for n in numbers}  # RUF025


def func():
    for key, value in {n: 1 for n in [1, 2, 3]}.items():  # RUF025
        pass


def func():
    {n: 1.1 for n in [1, 2, 3]}  # RUF025


def func():
    {n: complex(3, 5) for n in [1, 2, 3]}  # RUF025


def func():
    def f(data):
        return data

    f({c: "a" for c in "12345"})  # RUF025


def func():
    {n: True for n in [1, 2, 2]}  # RUF025


def func():
    {n: b"hello" for n in (1, 2, 2)}  # RUF025


def func():
    {n: ... for n in [1, 2, 3]}  # RUF025


def func():
    {n: False for n in {1: "a", 2: "b"}}  # RUF025


def func():
    {(a, b): 1 for (a, b) in [(1, 2), (3, 4)]}  # RUF025


def func():
    def f():
        return 1

    a = f()
    {n: a for n in [1, 2, 3]}  # RUF025


def func():
    values = ["a", "b", "c"]
    [{n: values for n in [1, 2, 3]}]  # RUF025


# Non-violation cases: RUF025


def func():
    {n: 1 for n in [1, 2, 3, 4, 5] if n < 3}  # OK


def func():
    {n: 1 for c in [1, 2, 3, 4, 5] for n in [1, 2, 3] if c < 3}  # OK


def func():
    def f():
        pass

    {n: f() for n in [1, 2, 3]}  # OK


def func():
    {n: n for n in [1, 2, 3, 4, 5]}  # OK


def func():
    def f():
        return {n: 1 for c in [1, 2, 3, 4, 5] for n in [1, 2, 3]}  # OK

    f()


def func():
    {(a, b): a + b for (a, b) in [(1, 2), (3, 4)]}  # OK

# https://github.com/astral-sh/ruff/issues/18764
{ # 1
a # 2
: # 3
None # 4
for # 5
a # 6
in # 7
iterable # 8
} # 9


class C:
    a = None

def func():
    {(C.a,): None for (C.a,) in "abc"}  # OK


def func():
    obj = type('obj', (), {'attr': 1})()
    {(obj.attr,): None for (obj.attr,) in "abc"}  # OK


def func():
    lst = [1, 2, 3]
    {(lst[0],): None for (lst[0],) in "abc"}  # OK


def func():
    lst = [1, 2, 3, 4, 5]
    {(lst[1:3],): None for (lst[1:3],) in "abc"}  # OK
