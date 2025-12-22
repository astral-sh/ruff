"""
The code here is not in an `__init__.py` file and should not trigger the
lint.
"""

# convenience imports:
import os
from pathlib import Path

__all__ = ["MY_CONSTANT"]
__all__ += ["foo"]
__all__: list[str] = __all__
__all__ = __all__ = __all__

MY_CONSTANT = 5
"""This is an important constant."""

os.environ["FOO"] = 1


def foo():
    return Path("foo.py")
