"""
Violation:

Checks for `raise` statements within `try` blocks.
"""
class MyException(Exception):
    pass


def bad():
    try:
        a = process()
        if not a:
            raise MyException(a)

        raise MyException(a)

        try:
            b = process()
            if not b:
                raise MyException(b)
        except Exception:
            logger.exception("something failed")
    except Exception:
        logger.exception("something failed")


def bad():
    try:
        a = process()
        if not a:
            raise MyException(a)

        raise MyException(a)

        try:
            b = process()
            if not b:
                raise MyException(b)
        except* Exception:
            logger.exception("something failed")
    except* Exception:
        logger.exception("something failed")


def good():
    try:
        a = process()  # This throws the exception now
    except MyException:
        logger.exception("a failed")
    except Exception:
        logger.exception("something failed")


def fine():
    try:
        a = process()  # This throws the exception now
    finally:
        print("finally")


def fine():
    try:
        raise ValueError("a doesn't exist")
    except TypeError: # A different exception is caught
        print("A different exception is caught")
