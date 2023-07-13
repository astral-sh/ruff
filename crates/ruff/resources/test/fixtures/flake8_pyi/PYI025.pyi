def f():
    from collections.abc import Set as AbstractSet  # Ok

def f():
    from collections.abc import Container, Sized, Set as AbstractSet, ValuesView  # Ok

def f():
    from collections.abc import Set  # PYI025

def f():
    from collections.abc import Container, Sized, Set, ValuesView  # PYI025

def f():
    """Test: local symbol renaming."""
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

from collections.abc import Set

def f():
    """Test: global symbol renaming."""
    global Set

    Set = 1
    print(Set)

def f():
    """Test: nonlocal symbol renaming."""
    from collections.abc import Set

    def g():
        nonlocal Set

        Set = 1
        print(Set)
