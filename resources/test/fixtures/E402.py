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

y = x + 1

import f


def foo() -> None:
    import e


if __name__ == "__main__":
    import g
