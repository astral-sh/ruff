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
