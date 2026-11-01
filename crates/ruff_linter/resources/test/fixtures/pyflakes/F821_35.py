# Regression test for https://github.com/astral-sh/ruff/issues/23364


class Foo:
    BAR = [1, 2, 3]
    BAZ = [4, 5, 6]

    # OK: a generator body runs when the generator is consumed, after the class is defined.
    a = ((x, y) for x in BAR for y in Foo.BAZ)
    b = (Foo.BAZ for x in BAR)

    # Error: a list comprehension is evaluated eagerly in the class body.
    c = [(x, y) for x in BAR for y in Foo.BAZ]

    # Error: the first iterable of a generator is also evaluated eagerly.
    d = (x for x in Foo.BAZ)

    # Error: a name that is never bound is still reported.
    e = (undefined for x in BAR)


class WalrusInLambda:
    values = (0,)

    # OK: the class name is bound by the time the generator is consumed.
    generated = ((WalrusInLambda, lambda: (seen := 1)) for _ in values)


try:
    raise ValueError("live")
except ValueError as exc:

    class ExceptionNameInClass:
        values = (0,)

        # OK: `tuple` consumes the generator while `exc` is still bound.
        rendered = tuple(str(exc) for _ in values)


class HandledNameError:
    # OK: the `NameError` is handled.
    try:
        values = tuple(undefined for _ in (0,))
    except NameError:
        values = ()


class NestedInComprehension:
    # OK: the comprehension creates the generators eagerly, but they run when consumed.
    generators = [(NestedInComprehension for _ in (0,)) for _ in (0,)]


class LaterTarget:
    # Error: `x` is bound by a later `for` clause, so it is read before it is assigned.
    generated = (x for _ in (0,) if x for x in (1,))


class LaterTargetInNestedGenerator:
    # Error: `any` consumes the inner generator before the outer `for` clause assigns `x`.
    generated = (0 for _ in (0,) if any(x for _ in (0,)) for x in (1,))
