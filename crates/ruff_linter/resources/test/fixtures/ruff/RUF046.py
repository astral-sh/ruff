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

int(1 and 0)
int(0 or -1)


if int(1 + 2) * 3:
    ...


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

int(foo if ... else 4)

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

int(1 +
    1)

int(round(1,
          0))

# function calls may need to retain parentheses
# if the parentheses for the call itself
# lie on the next line.
# See https://github.com/astral-sh/ruff/issues/15263
int(round
(1))

int(round # a comment
# and another comment
(10)
)

int(round (17)) # this is safe without parens

int( round (
                17
            )) # this is also safe without parens

int((round)  # Comment
(42)
)

int((round  # Comment
)(42)
)

int(  # Unsafe fix because of this comment
(  # Comment
    (round
)  # Comment
)(42)
)

int(
    round(
        42
    ) # unsafe fix because of this comment
)

int(
    round(
        42
    ) 
# unsafe fix because of this comment
)
