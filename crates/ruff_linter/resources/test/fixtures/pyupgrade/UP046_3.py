"""TypeVars with starred constraints cannot be translated to PEP 695 syntax.

`TypeVar("T", *constraints)` unpacks a runtime tuple, but PEP 695's `[T: (...)]`
syntax requires statically-known constraint expressions. The autofix would
otherwise produce invalid syntax like `[T: (*constraints)]`. The fix is
therefore suppressed; only the diagnostic is emitted. See issue #26954.
"""

from typing import Generic, TypeVar

constraints = (int, str)
T1 = TypeVar("T1", *constraints)


class GenericClass(Generic[T1]):
    var: T1


# Mixed: an ordinary constrained TypeVar and a starred-constraint TypeVar.
# The whole class bails (no fix) because T2 cannot be represented.
T2 = TypeVar("T2", *constraints)
S = TypeVar("S", str, bytes)


class Mixed(Generic[S, T2]):
    var: S
    other: T2
