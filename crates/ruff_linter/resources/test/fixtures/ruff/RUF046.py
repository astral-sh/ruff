import math


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

int(1)
int(v := 1)
int(~1)
int(-1)
int(+1)

int(1 + 1)
int(1 - 1)
int(1 * 1)
int(1 % 1)
int(1 ** 1)
int(1 << 1)
int(1 >> 1)
int(1 | 1)
int(1 ^ 1)
int(1 & 1)
int(1 // 1)

int(1 if ... else 2)

int(round(1))
int(round(1, None))
int(round(1, 0))


### Unsafe

int(math.ceil())
int(math.floor())
int(math.trunc())


### No errors

int(1 and 0)
int(0 or -1)

int(foo if ... else 4)

int(round())
int(round(ndigits=2))
int(round(3.4))
int(round(3.4, 0))
int(round(3.4, 2))
int(round(5, foo))

int(3.14)
int(2.8j)

async def f():
    int(await f())

int(foo.bar)
int(bar([1][False]))

int(1 == 1)
int(1 != 1)
int(1 < 1)
int(1 <= 1)
int(1 > 1)
int(1 >= 1)
int(1 in 1)
int(1 not in 1)
int(1 is 1)
int(2 is not 3)
int(foo in 1)
int(foo not in 1)
int(foo is 1)
int(foo is not 1)

int(1 == 2 == 3)
int(foo == 1)
int(foo != 1)
int(foo < 1)
int(foo <= 1)
int(foo > 1)
int(foo >= 1)

int(v := {}[{}['']])

int(foo + 1)
int(foo - 1)
int(foo * 1)
int(foo @ 1)
int(foo / 1)
int(foo % 1)
int(foo ** 1)
int(foo << 1)
int(foo >> 1)
int(foo | 1)
int(foo ^ 1)
int(foo & 1)
int(foo // 1)

int(v := 3.7)

int(not 109)

int(1 / 1)
int(1 @ 1)

int(1. if ... else .2)

int(round(5, 1))
