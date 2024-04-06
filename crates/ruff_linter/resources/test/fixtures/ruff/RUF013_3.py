import typing


def f(arg: typing.List[str] = None):  # RUF013
    pass


# Optional


def f(arg: typing.Optional[int] = None):
    pass


# Union


def f(arg: typing.Union[int, str, None] = None):
    pass


def f(arg: typing.Union[int, str] = None):  # RUF013
    pass


# Literal


def f(arg: typing.Literal[1, "foo", True] = None):  # RUF013
    pass
