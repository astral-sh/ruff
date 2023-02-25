"""
Violation:

Too many try/except blocks.
Keep it short to 1 per function.
"""


def func():
    try:
        a = 1
        print(a)
    except Exception:
        raise

    try:
        b = 2
        print(b)
    except Exception:
        raise


def nested_func():
    try:
        try:
            b = 2
            print(b)
        except Exception:
            raise
    except Exception:
        raise


def func_two():
    try:
        a = 1
        print(a)
    except Exception:
        raise

    try:
        b = 2
        print(b)
    except Exception:
        raise

    try:
        c = 3
        print(c)
    except Exception:
        raise


def func_good():
    try:
        print(1)
    except Exception:
        raise

    if True:
        print(2)
