def f():
    from collections.abc import Set as AbstractSet  # Ok

def f():
    from collections.abc import Container, Sized, Set as AbstractSet, ValuesView  # Ok

def f():
    from collections.abc import Set  # PYI025

def f():
    from collections.abc import Container, Sized, Set, ValuesView  # PYI025

def f():
    if True:
        from collections.abc import Set
    else:
        Set = 1

    x: Set = set()

    x: Set

    del Set

    def f():
        print(Set)

        def Set():
            pass
        print(Set)
