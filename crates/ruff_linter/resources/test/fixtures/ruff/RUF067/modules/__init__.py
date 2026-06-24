"""This is the module docstring."""

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

# any dunder-named assignment is allowed in non-strict mode
__path__ = 5  # ok
__submodules__ = []  # ok (e.g. mkinit)
__protected__ = []  # ok
__custom__: list[str] = []  # ok
__submodules__ += ["extra"]  # ok

foo = __submodules__ = []  # RUF067: not every target is a dunder
__all__[0] = __version__ = "1"  # RUF067: subscript target is not a simple name

# also allow `__author__`
__author__ = "The Author"  # ok
