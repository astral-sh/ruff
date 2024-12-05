import math


### Safely fixable

# Arguments are not checked
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


### Unsafe

int(math.ceil())
int(math.floor())
int(math.trunc())


### `round()`

## Errors
int(round(0))
int(round(0, 0))
int(round(0, None))

int(round(0.1))
int(round(0.1, None))

# Argument type is not checked
foo = type("Foo", (), {"__round__": lambda self: 4.2})()

int(round(foo))
int(round(foo, 0))
int(round(foo, None))

## No errors
int(round(0, 3.14))
int(round(0, non_literal))
int(round(0, 0), base)
int(round(0, 0, extra=keyword))
int(round(0.1, 0))
