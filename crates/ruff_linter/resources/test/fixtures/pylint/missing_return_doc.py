def return_good(a: int, b: int) -> int:
    """Returns sum of two integers.
    :param a: first integer
    :param b: second integer
    :return: sum of parameters a and b
    """
    return a + b


def no_return_good(a: int, b: int) -> None:
    """Prints the sum of two integers.
    :param a: first integer
    :param b: second integer
    """
    print(a + b)


def no_return_also_good(a: int, b: int) -> None:
    """Prints the sum of two integers.
    :param a: first integer
    :param b: second integer
    """
    print(a + b)
    return None


def return_bad(a: int, b: int):  # [missing-return-doc]
    """Returns sum of two integers.
    :param a: first integer
    :param b: second integer
    """
    return a + b
