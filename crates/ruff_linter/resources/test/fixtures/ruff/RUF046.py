import math


inferred_int = 1
inferred_float = 1.


### Safely fixable

int(id())
int(len([]))
int(ord(foo))
int(hash(foo, bar))
int(int(''))

int(math.comb())
int(math.factorial())
int(math.gcd())
int(math.lcm())
int(math.isqrt())
int(math.perm())

int(round(1, 0))
int(round(1, 10))

int(round(1))
int(round(1, None))

int(round(1.))
int(round(1., None))


### Unsafe

int(math.ceil())
int(math.floor())
int(math.trunc())

int(round(inferred_int, 0))
int(round(inferred_int, 10))

int(round(inferred_int))
int(round(inferred_int, None))

int(round(inferred_float))
int(round(inferred_float, None))

int(round(unknown))
int(round(unknown, None))


### No errors

int(round(1, unknown))
int(round(1., unknown))

int(round(1., 0))
int(round(inferred_float, 0))

int(round(inferred_int, unknown))
int(round(inferred_float, unknown))

int(round(unknown, 0))
int(round(unknown, unknown))

int(round(0, 3.14))
int(round(inferred_int, 3.14))

int(round(0, 0), base)
int(round(0, 0, extra=keyword))
