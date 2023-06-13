from collections.abc import Set as AbstractSet  # Ok

from collections.abc import Container, Sized, Set as AbstractSet, ValuesView  # Ok

from collections.abc import Set  # PYI025

from collections.abc import Container, Sized, Set, ValuesView  # PYI025

GLOBAL: Set[int] = set()

class Class:
    member: Set[int]
