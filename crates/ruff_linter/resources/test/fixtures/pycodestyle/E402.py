"""Top-level docstring."""

__all__ = ["y"]
__version__: str = "0.1.0"

import a

try:
    import b
except ImportError:
    pass
else:
    pass

import c

if x > 0:
    import d
else:
    import e

import sys
sys.path.insert(0, "some/path")

import f

__some__magic = 1

import g


def foo() -> None:
    import h


if __name__ == "__main__":
    import i

import j; import k


if __name__ == "__main__":
    import l; \
import m
