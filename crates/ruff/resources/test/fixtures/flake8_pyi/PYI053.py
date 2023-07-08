def f1(x: str = "50 character stringggggggggggggggggggggggggggggggg") -> None:
    ...


def f2(x: str = "51 character stringgggggggggggggggggggggggggggggggg") -> None:
    ...


def f3(x: str = "50 character stringggggggggggggggggggggggggggggg\U0001f600") -> None:
    ...


def f4(x: str = "51 character stringgggggggggggggggggggggggggggggg\U0001f600") -> None:
    ...


def f5(x: bytes = b"50 character byte stringgggggggggggggggggggggggggg") -> None:
    ...


def f6(x: bytes = b"51 character byte stringgggggggggggggggggggggggggg") -> None:
    ...


def f7(x: bytes = b"50 character byte stringggggggggggggggggggggggggg\xff") -> None:
    ...


def f8(x: bytes = b"50 character byte stringgggggggggggggggggggggggggg\xff") -> None:
    ...


foo: str = "50 character stringggggggggggggggggggggggggggggggg"
bar: str = "51 character stringgggggggggggggggggggggggggggggggg"

baz: bytes = b"50 character byte stringgggggggggggggggggggggggggg"

qux: bytes = b"51 character byte stringggggggggggggggggggggggggggg\xff"


class Demo:
    """Docstrings are excluded from this rule. Some padding."""


def func() -> None:
    """Docstrings are excluded from this rule. Some padding."""
