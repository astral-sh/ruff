"""
Should emit:
B002 - on lines 18, 19, and 24
"""


def this_is_all_fine(n):
    x = n + 1
    y = 1 + n
    z = +x + y
    a = n - 1
    b = 1 - n
    c = -a - b
    return +z, -c


def this_is_buggy(n):
    x = ++n
    y = --n
    return x, y


def this_is_buggy_too(n):
    return ++n, --n
