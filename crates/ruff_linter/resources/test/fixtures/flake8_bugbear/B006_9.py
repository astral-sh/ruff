def f1(x=([],)):
    print(x)


def f2(x=(x for x in "x")):
    print(x)


def f3(x=((x for x in "x"),)):
    print(x)


def f4(x=(z := [1, ])):
    print(x)


def f5(x=([1, ])):
    print(x)


def w1(x=(1,)):
    print(x)


def w2(x=(z := 3)):
    print(x)


# A multiline string literal must keep its exact runtime value: re-indenting the
# generated `if` block must not indent the string's own continuation lines.
# https://github.com/astral-sh/ruff/issues/27022
def m1(x=["""first
second"""]):
    print(x)


def m2(x=("""a
b""",)):
    print(x)


class C:
    def method(self, x=["""line1
line2"""]):
        print(x)
