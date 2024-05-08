"""__init__.py with __all__ populated by conditional plus-eq
"""

import sys

from . import exported, renamed as bees

if sys.version_info > (3, 9):
    from . import also_exported

__all__ = ["exported"]

if sys.version_info >= (3, 9):
    __all__ += ["also_exported"]
