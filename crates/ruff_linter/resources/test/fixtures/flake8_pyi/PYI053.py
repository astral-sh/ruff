import warnings
import typing_extensions
from collections.abc import Callable
from warnings import deprecated


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
baz: str = f"51 character stringgggggggggggggggggggggggggggggggg"

baz: bytes = b"50 character byte stringgggggggggggggggggggggggggg"

qux: bytes = b"51 character byte stringggggggggggggggggggggggggggg\xff"


class Demo:
    """Docstrings are excluded from this rule. Some padding."""


def func() -> None:
    """Docstrings are excluded from this rule. Some padding."""


@warnings.deprecated("Veeeeeeeeeeeeeeeeeeeeeeery long deprecation message, but that's okay")
def deprecated_function() -> None: ...


@typing_extensions.deprecated("Another loooooooooooooooooooooong deprecation message, it's still okay")
def another_deprecated_function() -> None: ...


@deprecated("A third loooooooooooooooooooooooooooooong deprecation message")
def a_third_deprecated_function() -> None: ...


def not_warnings_dot_deprecated(
    msg: str
) -> Callable[[Callable[[], None]], Callable[[], None]]: ...


@not_warnings_dot_deprecated("Not warnings.deprecated, so this one *should* lead to PYI053 in a stub!")
def not_a_deprecated_function() -> None: ...
