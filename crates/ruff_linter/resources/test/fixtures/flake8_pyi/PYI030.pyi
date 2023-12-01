import typing
import typing_extensions
from typing import Literal

# Shouldn't affect non-union field types.
field1: Literal[1]  # OK

# Should emit for duplicate field types.
field2: Literal[1] | Literal[2]  # Error

# Should emit for union types in arguments.
def func1(arg1: Literal[1] | Literal[2]):  # Error
    print(arg1)


# Should emit for unions in return types.
def func2() -> Literal[1] | Literal[2]:  # Error
    return "my Literal[1]ing"


# Should emit in longer unions, even if not directly adjacent.
field3: Literal[1] | Literal[2] | str  # Error
field4: str | Literal[1] | Literal[2]  # Error
field5: Literal[1] | str | Literal[2]  # Error
field6: Literal[1] | bool | Literal[2] | str  # Error

# Should emit for non-type unions.
field7 = Literal[1] | Literal[2]  # Error

# Should emit for parenthesized unions.
field8: Literal[1] | (Literal[2] | str)  # Error

# Should handle user parentheses when fixing.
field9: Literal[1] | (Literal[2] | str)  # Error
field10: (Literal[1] | str) | Literal[2]  # Error

# Should emit for union in generic parent type.
field11: dict[Literal[1] | Literal[2], str]  # Error

# Should emit for unions with more than two cases
field12: Literal[1] | Literal[2] | Literal[3]  # Error
field13: Literal[1] | Literal[2] | Literal[3] | Literal[4]  # Error

# Should emit for unions with more than two cases, even if not directly adjacent
field14: Literal[1] | Literal[2] | str | Literal[3]  # Error

# Should emit for unions with mixed literal internal types
field15: Literal[1] | Literal["foo"] | Literal[True]  # Error

# Shouldn't emit for duplicate field types with same value; covered by Y016
field16: Literal[1] | Literal[1]  # OK

# Shouldn't emit if in new parent type
field17: Literal[1] | dict[Literal[2], str]  # OK

# Shouldn't emit if not in a union parent
field18: dict[Literal[1], Literal[2]]  # OK

# Should respect name of literal type used
field19: typing.Literal[1] | typing.Literal[2]  # Error

# Should emit in cases with newlines
field20: typing.Union[
    Literal[
        1  # test
    ],
    Literal[2],
]  # Error, newline and comment will not be emitted in message

# Should handle multiple unions with multiple members
field21: Literal[1, 2] | Literal[3, 4]  # Error

# Should emit in cases with `typing.Union` instead of `|`
field22: typing.Union[Literal[1], Literal[2]]  # Error

# Should emit in cases with `typing_extensions.Literal`
field23: typing_extensions.Literal[1] | typing_extensions.Literal[2]  # Error

# Should emit in cases with nested `typing.Union`
field24: typing.Union[Literal[1], typing.Union[Literal[2], str]]  # Error

# Should emit in cases with mixed `typing.Union` and `|`
field25: typing.Union[Literal[1], Literal[2] | str]  # Error

# Should emit only once in cases with multiple nested `typing.Union`
field24: typing.Union[Literal[1], typing.Union[Literal[2], typing.Union[Literal[3], Literal[4]]]]  # Error

# Should use the first literal subscript attribute when fixing
field25: typing.Union[typing_extensions.Literal[1], typing.Union[Literal[2], typing.Union[Literal[3], Literal[4]]], str]  # Error
