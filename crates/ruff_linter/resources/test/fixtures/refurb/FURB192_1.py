# A `yield` expression is only valid as a call argument when it is
# parenthesized, so the parentheses have to be preserved by the fix.
#
# These live in their own fixture because `FURB192.py` shadows `sorted` with a
# module-level function, which suppresses the rule inside function bodies.


def f(l, key_fn):
    sorted((yield))[0]

    sorted((yield l))[-1]

    sorted((yield), key=key_fn)[0]

    sorted((yield from l))[0]

    sorted((yield), reverse=True)[-1]
