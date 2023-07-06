import typing
from typing import Literal

# Shouldn't affect non-union field types.
field1: Literal[1]  # OK

# Should emit for duplicate field types.
field2: Literal[1] | Literal[2]  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `Literal[1, 2]".

# Should emit for union types in arguments.
def func1(arg1: Literal[1] | Literal[2]):  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `Literal[1, 2]".
    print(arg1)


# Should emit for unions in return types.
def func2() -> Literal[1] | Literal[2]:  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `Literal[1, 2]".
    return "my Literal[1]ing"


# Should emit in longer unions, even if not directly adjacent.
field3: Literal[1] | Literal[2] | str  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `Literal[1, 2]".
field4: str | Literal[1] | Literal[2]  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `Literal[1, 2]".
field5: Literal[1] | str | Literal[2]  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `Literal[1, 2]".
field6: Literal[1] | bool | Literal[2] | str  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `Literal[1, 2]".

# Should emit for non-type unions.
field7 = Literal[1] | Literal[2]  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `Literal[1, 2]".

# Should emit for parenthesized unions.
field8: Literal[1] | (Literal[2] | str)  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `Literal[1, 2]".

# Should handle user parentheses when fixing.
field9: Literal[1] | (Literal[2] | str)  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `Literal[1, 2]".
field10: (Literal[1] | str) | Literal[2]  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `Literal[1, 2]".

# Should emit for union in generic parent type.
field11: dict[Literal[1] | Literal[2], str]  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `Literal[1, 2]".

# Should emit for unions with more than two cases
field12: Literal[1] | Literal[2] | Literal[3]  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `Literal[1, 2, 3]".
field13: Literal[1] | Literal[2] | Literal[3] | Literal[4]  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `Literal[1, 2, 3, 4]".

# Should emit for unions with more than two cases, even if not directly adjacent
field14: Literal[1] | Literal[2] | str | Literal[3]  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `Literal[1, 2, 3]".

# Should emit for unions with mixed literal internal types
field15: Literal[1] | Literal["foo"] | Literal[True]  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `Literal[1, "foo", True]".

# Shouldn't emit for duplicate field types with same value; covered by Y016
field16: Literal[1] | Literal[1]  # Error: Y016 Duplicate union member "Literal[1]"

# Shouldn't emit if in new parent type
field17: Literal[1] | dict[Literal[2], str]  # OK

# Shouldn't emit if not in a union parent
field18: dict[Literal[1], Literal[2]]  # OK

# Should respect name of literal type used
field19: typing.Literal[1] | typing.Literal[2]  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `typing.Literal[1, 2]".

# Should handle newlines
field20: typing.Union[
    Literal[
        1  # test
    ],
    Literal[2],
]  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `typing.Union[typing.Literal[1], typing.Literal[2]]".

# Should handle multiple unions with multiple members
field16: Literal[1, 2] | Literal[3, 4]  # Error: PYI030 Multiple literal members in a union. Use a single literal e.g. `typing.Literal[1, 2, 3, 4]".