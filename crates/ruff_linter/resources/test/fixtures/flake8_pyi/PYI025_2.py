"""Tests to ensure we correctly rename references inside `__all__`"""

from collections.abc import Set

__all__ = ["Set"]

if True:
    __all__ += [r'''Set''']

if 1:
    __all__ += ["S" "e" "t"]

if not False:
    __all__ += ["Se" 't']
