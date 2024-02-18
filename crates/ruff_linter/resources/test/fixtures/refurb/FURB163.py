import math

from math import e as special_e
from math import log as special_log

# Errors.
math.log(1, 2)
math.log(1, 10)
math.log(1, math.e)
foo = ...
math.log(foo, 2)
math.log(foo, 10)
math.log(foo, math.e)
math.log(1, special_e)
special_log(1, 2)
special_log(1, 10)
special_log(1, math.e)
special_log(1, special_e)
math.log(1, 2.0)
math.log(1, 10.0)

# Ok.
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
