class CustomException(Exception):
    pass


def func():
    a = 1
    if a == 1:
        raise CustomException("Long message")
    elif a == 2:
        raise CustomException("Short")  # This is acceptable
    elif a == 3:
        raise CustomException("its_code_not_message")  # This is acceptable


def ignore():
    try:
        a = 1
    except Exception as ex:
        # This is another violation, but this specific analyzer shouldn't care
        raise ex


class BadArgCantBeEven(Exception):
    pass


class GoodArgCantBeEven(Exception):
    def __init__(self, arg):
        super().__init__(f"The argument '{arg}' should be even")


def bad(a):
    if a % 2 == 0:
        raise BadArgCantBeEven(f"The argument '{a}' should be even")


def another_bad(a):
    if a % 2 == 0:
        raise BadArgCantBeEven(f"The argument {a} should not be odd.")


def and_another_bad(a):
    if a % 2 == 0:
        raise BadArgCantBeEven("The argument `a` should not be odd.")


def good(a: int):
    if a % 2 == 0:
        raise GoodArgCantBeEven(a)


def another_good(a):
    if a % 2 == 0:
        raise GoodArgCantBeEven(a)
