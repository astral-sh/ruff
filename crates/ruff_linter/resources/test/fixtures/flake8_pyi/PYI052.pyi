import builtins
import typing
from typing import TypeAlias, Final, NewType, TypeVar, TypeVarTuple, ParamSpec

# We shouldn't emit Y015 for simple default values
field1: int
field2: int = ...
field3 = ...  # type: int  # Y033 Do not use type comments in stubs (e.g. use "x: int" instead of "x = ... # type: int")
field4: int = 0
field41: int = 0xFFFFFFFF
field42: int = 1234567890
field43: int = -0xFFFFFFFF
field44: int = -1234567890
field5 = 0  # type: int  # Y033 Do not use type comments in stubs (e.g. use "x: int" instead of "x = ... # type: int")  # Y052 Need type annotation for "field5"
field6 = 0  # Y052 Need type annotation for "field6"
field7 = b""  # Y052 Need type annotation for "field7"
field71 = "foo"  # Y052 Need type annotation for "field71"
field72: str = "foo"
field8 = False  # Y052 Need type annotation for "field8"
field81 = -1  # Y052 Need type annotation for "field81"
field82: float = -98.43
field83 = -42j  # Y052 Need type annotation for "field83"
field84 = 5 + 42j  # Y052 Need type annotation for "field84"
field85 = -5 - 42j  # Y052 Need type annotation for "field85"
field9 = None  # Y026 Use typing_extensions.TypeAlias for type aliases, e.g. "field9: TypeAlias = None"
Field95: TypeAlias = None
Field96: TypeAlias = int | None
Field97: TypeAlias = None | typing.SupportsInt | builtins.str | float | bool
Field98 = NewType('MyInt', int)
Field99 = TypeVar('Field99')
Field100 = TypeVarTuple('Field100')
Field101 = ParamSpec('Field101')
field19 = [1, 2, 3]  # Y052 Need type annotation for "field19"
field191: list[int] = [1, 2, 3]
field20 = (1, 2, 3)  # Y052 Need type annotation for "field20"
field201: tuple[int, ...] = (1, 2, 3)
field21 = {1, 2, 3}  # Y052 Need type annotation for "field21"
field211: set[int] = {1, 2, 3}
field212 = {"foo": "bar"}  # Y052 Need type annotation for "field212"
field213: dict[str, str] = {"foo": "bar"}
field22: Final = {"foo": 5}

# We *should* emit Y015 for more complex default values
field221: list[int] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]  # Y015 Only simple default values are allowed for assignments
field223: list[int] = [*range(10)]  # Y015 Only simple default values are allowed for assignments
field224: list[int] = list(range(10))  # Y015 Only simple default values are allowed for assignments
field225: list[object] = [{}, 1, 2]  # Y015 Only simple default values are allowed for assignments
field226: tuple[str | tuple[str, ...], ...] = ("foo", ("foo", "bar"))  # Y015 Only simple default values are allowed for assignments
field227: dict[str, object] = {"foo": {"foo": "bar"}}  # Y015 Only simple default values are allowed for assignments
field228: dict[str, list[object]] = {"foo": []}  # Y015 Only simple default values are allowed for assignments
# When parsed, this case results in `None` being placed in the `.keys` list for the `ast.Dict` node
field229: dict[int, int] = {1: 2, **{3: 4}}  # Y015 Only simple default values are allowed for assignments
field23 = "foo" + "bar"  # Y015 Only simple default values are allowed for assignments
field24 = b"foo" + b"bar"  # Y015 Only simple default values are allowed for assignments
field25 = 5 * 5  # Y015 Only simple default values are allowed for assignments

# We shouldn't emit Y015 within functions
def f():
  field26: list[int] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]


# We shouldn't emit Y015 for __slots__ or __match_args__
class Class1:
  __slots__ = (
    '_one',
    '_two',
    '_three',
    '_four',
    '_five',
    '_six',
    '_seven',
    '_eight',
    '_nine',
    '_ten',
    '_eleven',
  )

  __match_args__ = (
    'one',
    'two',
    'three',
    'four',
    'five',
    'six',
    'seven',
    'eight',
    'nine',
    'ten',
    'eleven',
  )

# We shouldn't emit Y015 for __all__
__all__ = ["Class1"]

# Ignore the following for PYI015
field26 = typing.Sequence[int]
field27 = list[str]
field28 = builtins.str
field29 = str
field30 = str | bytes | None

# We shouldn't emit Y052 for `enum` subclasses.
from enum import Enum

class Foo(Enum):
    FOO = 0
    BAR = 1

class Bar(Foo):
    BAZ = 2
    BOP = 3

class Bop:
    WIZ = 4
