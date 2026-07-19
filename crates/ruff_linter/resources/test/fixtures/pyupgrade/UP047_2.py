"""TypeVars with starred constraints cannot be translated to PEP 695 syntax.

`TypeVar("T", *constraints)` unpacks a runtime tuple, but PEP 695's `[T: (...)]`
syntax requires statically-known constraint expressions. The autofix would
otherwise produce invalid syntax like `[T: (*constraints)]`. The fix is
therefore suppressed; only the diagnostic is emitted. See issue #26954.
"""

from typing import TypeVar

constraints = (int, str)
T1 = TypeVar("T1", *constraints)


def generic_function(var: T1) -> T1:
    return var


# Mixed: an ordinary bound TypeVar and a starred-constraint TypeVar.
# The whole function bails (no fix) because T2 cannot be represented.
T2 = TypeVar("T2", *constraints)
T = TypeVar("T", bound=float)


def mixed_function(t: T, x: T2) -> tuple[T, T2]:
    return (t, x)
