"""Test that `__all__` exports are respected even with multiple declarations."""

import random


def some_dependency_check():
    return random.uniform(0.0, 1.0) > 0.49999


if some_dependency_check():
    import math

    __all__ = ["math"]
else:
    __all__ = []
