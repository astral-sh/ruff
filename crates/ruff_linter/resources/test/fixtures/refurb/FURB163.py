import math

# Errors
math.log(1, 2)
math.log(1, 10)
math.log(1, math.e)
foo = ...
math.log(foo, 2)
math.log(foo, 10)
math.log(foo, math.e)
math.log(1, 2.0)
math.log(1, 10.0)

# OK
math.log2(1)
math.log10(1)
math.log(1)
math.log(1, 3)
math.log(1, math.pi)

two = 2
math.log(1, two)

ten = 10
math.log(1, ten)

e = math.e
math.log(1, e)

math.log2(1, 10)  # math.log2 takes only one argument.
math.log10(1, 2)  # math.log10 takes only one argument.

math.log(1, base=2)  # math.log does not accept keyword arguments.


def log(*args):
    print(f"Logging: {args}")


log(1, 2)
log(1, 10)
log(1, math.e)

math.log(1, 2.0001)
math.log(1, 10.0001)

# https://github.com/astral-sh/ruff/issues/18747
def log():
    yield math.log((yield), math.e)


def log():
    yield math.log((yield from x), math.e)

# see: https://github.com/astral-sh/ruff/issues/18639
math.log(1, 10 # comment
         )

math.log(1,
         10 # comment
         )

math.log(1 # comment
         , # comment
         10 # comment
         )

math.log(
    1 # comment
    ,
    10 # comment
)

math.log(4.13e223, 2)
math.log(4.14e223, 10)


def print_log(*args):
    try:
        print(math.log(*args, math.e))
    except TypeError as e:
        print(repr(e))
