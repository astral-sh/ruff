def shadowing_function():
    def str(x):
        return x

    from builtins import str  # okay, `str` is otherwise shadowed

    str(1)


def non_star_import():
    from math import pow
    from builtins import pow  # okay, `pow` is otherwise shadowed

    pow(1, 1, 1)


def star_import():
    from math import *
    from builtins import pow  # UP029, false positive due to the star import

    pow(1, 1, 1)


def unsafe_fix():
    from builtins import (
        str,  # UP029, removes this comment
    )

    str(1)
