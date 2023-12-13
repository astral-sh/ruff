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

import matplotlib

matplotlib.use("Agg")

import g

__some__magic = 1

import h


def foo() -> None:
    import i


if __name__ == "__main__":
    import j

import k; import l


if __name__ == "__main__":
    import m; \
import n
