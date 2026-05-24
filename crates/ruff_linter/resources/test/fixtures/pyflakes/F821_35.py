# Regression test for https://github.com/astral-sh/ruff/issues/23364


class Foo:
    BAR = [1, 2, 3]
    BAZ = [4, 5, 6]

    # No F821: a generator body and its non-first iterables are evaluated after the
    # class is defined, so `Foo` is bound by then.
    a = ((x, y) for x in BAR for y in Foo.BAZ)
    b = (Foo.BAZ for x in BAR)

    # F821: a list comprehension is evaluated eagerly in the class body.
    c = [(x, y) for x in BAR for y in Foo.BAZ]

    # F821: the first iterable of a generator is also evaluated eagerly.
    d = (x for x in Foo.BAZ)

    # F821: a name that is never bound is still reported in a deferred generator body.
    e = (undefined for x in BAR)

    def method(self):
        # No F821: the generator runs when consumed, after `Foo` is bound.
        return (Foo.BAZ for _ in range(3))


def uses_walrus(items):
    # No F821: an assignment expression binds `seen` in this scope (PEP 572), so the
    # generator is evaluated eagerly and `seen` is visible afterward.
    matches = (x for x in items if (seen := x) > 1)
    return seen, matches
