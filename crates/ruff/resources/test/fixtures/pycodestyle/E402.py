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

__some__magic = 1

import f


def foo() -> None:
    import e


if __name__ == "__main__":
    import g

import h; import i


if __name__ == "__main__":
    import j; \
import k
