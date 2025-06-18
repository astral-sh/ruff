from functools import reduce
from itertools import chain
from operator import add, concat, sub

s = "123456789"
b = b"123456789"
t = ((1, 2, 3), (4, 5, 6), (7, 8, 9))

l = [(1, 2, 3), (4, 5, 6), (7, 8, 9)]
e = {(1, 2, 3), (4, 5, 6), (7, 8, 9)}
d = {(1, 2, 3): 0, (4, 5, 6): 1, (7, 8, 9): 2}
i = iter(l)


### Errors


chain(*s)
chain(*b)
chain(*t)
reduce(add, s, ())
reduce(concat, b, tuple[str, int, list[bytes]]())
reduce(add, t, [])


chain(*l)
chain(*e)
chain(*d)
chain(*i)
reduce(concat, l, [])
reduce(add, e, ())
reduce(concat, d, tuple())
reduce(add, i, list[str]())


((  # Comment
    chain
)  # Comment

(  # Comment
    *  # Comment
    (  # Comment
        s
    )  # Comment
)  # Comment
)

_ = (
    c
    for b in l
    for c in b
)

_ = [
    (b, c)
    for a in l
    for b, c in a
]

_ = print(  # Preserved
    {  # Comment
        a for b in c for a in b
    }
)


def _():
    set = {0}
    _ = {c for b in a for c in b}


### No errors

from typing_extensions import Annotated

sum(l, [])  # Already caught by RUF017

chain(s)
chain(*b, foo = "bar")
chain.from_iterable(*t)
chain(*s, *b)

reduce(sub, l)
reduce(concat)
reduce(add, b, Annotated[tuple[str], ...]())
