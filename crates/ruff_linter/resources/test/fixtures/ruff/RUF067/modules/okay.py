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

def __getattr__(name):  # ok
    return name

__path__ = __import__('pkgutil').extend_path(__path__, __name__)  # ok

if os.environ["FOO"] != "1":  # RUF067
    MY_CONSTANT = 4  # ok, don't flag nested statements

if TYPE_CHECKING:  # ok
    MY_CONSTANT = 3

import typing

if typing.TYPE_CHECKING: # ok
    MY_CONSTANT = 2

__version__ = "1.2.3"  # ok

def __dir__():  # ok
    return ["foo"]

import pkgutil

__path__ = pkgutil.extend_path(__path__, __name__)  # ok
__path__ = unknown.extend_path(__path__, __name__)  # also ok

# non-`extend_path` assignments are not allowed
__path__ = 5  # RUF067

# also allow `__author__`
__author__ = "The Author"  # ok
