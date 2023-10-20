def f():
    "Here's a line without a period"
    ...


def f():
    """Here's a line without a period"""
    ...


def f():
    """
    Here's a line without a period,
    but here's the next line
    """
    ...


def f():
    """Here's a line without a period"""
    ...


def f():
    """
    Here's a line without a period,
    but here's the next line"""
    ...


def f():
    """
    Here's a line without a period,
    but here's the next line with trailing space """
    ...


def f():
    r"Here's a line without a period"
    ...


def f():
    r"""Here's a line without a period"""
    ...


def f():
    r"""
    Here's a line without a period,
    but here's the next line
    """
    ...


def f():
    r"""Here's a line without a period"""
    ...


def f():
    r"""
    Here's a line without a period,
    but here's the next line"""
    ...


def f():
    r"""
    Here's a line without a period,
    but here's the next line with trailing space """
    ...


def f(rounds: list[int], number: int) -> bool:
    """
    :param rounds: list - rounds played.
    :param number: int - round number.
    :return:  bool - was the round played?
    """
    return number in rounds


def f(rounds: list[int], number: int) -> bool:
    """
    Args:
        rounds (list): rounds played.
        number (int): round number.

    Returns:
        bool: was the round played?
    """
    return number in rounds
