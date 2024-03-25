from typing import Annotated, Any, Literal, Optional, Tuple, Union, Hashable


def f(arg: int):
    pass


def f(arg=None):
    pass


def f(arg: Any = None):
    pass


def f(arg: object = None):
    pass


def f(arg: int = None):  # RUF013
    pass


def f(arg: str = None):  # RUF013
    pass


def f(arg: Tuple[str] = None):  # RUF013
    pass


# Optional


def f(arg: Optional[int] = None):
    pass


# Union


def f(arg: Union[None] = None):
    pass


def f(arg: Union[None, int] = None):
    pass


def f(arg: Union[str, None] = None):
    pass


def f(arg: Union[int, str, Any] = None):
    pass


def f(arg: Union = None):  # RUF013
    pass


def f(arg: Union[int] = None):  # RUF013
    pass


def f(arg: Union[int, str] = None):  # RUF013
    pass


# PEP 604 Union


def f(arg: None | int = None):
    pass


def f(arg: int | None = None):
    pass


def f(arg: int | float | str | None = None):
    pass


def f(arg: int | float = None):  # RUF013
    pass


def f(arg: int | float | str | bytes = None):  # RUF013
    pass


# Literal


def f(arg: None = None):
    pass


def f(arg: Literal[None] = None):
    pass


def f(arg: Literal[1, 2, None, 3] = None):
    pass


def f(arg: Literal[1] = None):  # RUF013
    pass


def f(arg: Literal[1, "foo"] = None):  # RUF013
    pass


# Annotated


def f(arg: Annotated[Optional[int], ...] = None):
    pass


def f(arg: Annotated[Union[int, None], ...] = None):
    pass


def f(arg: Annotated[Any, ...] = None):
    pass


def f(arg: Annotated[int, ...] = None):  # RUF013
    pass


def f(arg: Annotated[Annotated[int | str, ...], ...] = None):  # RUF013
    pass


# Multiple arguments


def f(
    arg1: Optional[int] = None,
    arg2: Union[int, None] = None,
    arg3: Literal[1, 2, None, 3] = None,
):
    pass


def f(
    arg1: int = None,  # RUF013
    arg2: Union[int, float] = None,  # RUF013
    arg3: Literal[1, 2, 3] = None,  # RUF013
):
    pass


# Nested


def f(arg: Literal[1, "foo", Literal[True, None]] = None):
    pass


def f(arg: Union[int, Union[float, Union[str, None]]] = None):
    pass


def f(arg: Union[int, Union[float, Optional[str]]] = None):
    pass


def f(arg: Union[int, Literal[True, None]] = None):
    pass


def f(arg: Union[Annotated[int, ...], Annotated[Optional[float], ...]] = None):
    pass


def f(arg: Union[Annotated[int, ...], Union[str, bytes]] = None):  # RUF013
    pass


# Quoted


def f(arg: "int" = None):  # RUF013
    pass


def f(arg: "str" = None):  # RUF013
    pass


def f(arg: "st" "r" = None):  # RUF013
    pass


def f(arg: "Optional[int]" = None):
    pass


def f(arg: Union["int", "str"] = None):  # RUF013
    pass


def f(arg: Union["int", "None"] = None):
    pass


def f(arg: Union["No" "ne", "int"] = None):
    pass


# Avoid flagging when there's a parse error in the forward reference
def f(arg: Union["<>", "int"] = None):
    pass


# Type aliases

Text = str | bytes


def f(arg: Text = None):
    pass


def f(arg: "Text" = None):
    pass


from custom_typing import MaybeInt


def f(arg: MaybeInt = None):
    pass


# Hashable

def f(arg: Hashable = None):  # OK
    pass


def f(arg: Hashable | int = None):  # OK
    pass
