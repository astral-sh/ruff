def f():
    from collections.abc import Set as AbstractSet  # Ok


def f():
    from collections.abc import Container, Sized, Set as AbstractSet, ValuesView  # Ok


def f():
    from collections.abc import Set  # PYI025


def f():
    from collections.abc import Container, Sized, Set, ValuesView  # PYI025

    GLOBAL: Set[int] = set()

    class Class:
        member: Set[int]
