"""
Violation:
Reraise without using 'from'
"""


class MyException(Exception):
    pass


def func():
    try:
        a = 1
    except Exception:
        raise MyException()


def func():
    try:
        a = 1
    except Exception:
        if True:
            raise MyException()


def good():
    try:
        a = 1
    except MyException as e:
        raise e  # This is verbose violation, shouldn't trigger no cause
    except Exception:
        raise  # Just re-raising don't need 'from'
