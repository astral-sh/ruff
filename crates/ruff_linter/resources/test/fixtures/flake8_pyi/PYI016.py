import typing

# Shouldn't affect non-union field types.
field1: str

# Should emit for duplicate field types.
field2: str | str  # PYI016: Duplicate union member `str`

# Should emit for union types in arguments.
def func1(arg1: int | int):  # PYI016: Duplicate union member `int`
    print(arg1)

# Should emit for unions in return types.
def func2() -> str | str:  # PYI016: Duplicate union member `str`
    return "my string"

# Should emit in longer unions, even if not directly adjacent.
field3: str | str | int  # PYI016: Duplicate union member `str`
field4: int | int | str  # PYI016: Duplicate union member `int`
field5: str | int | str  # PYI016: Duplicate union member `str`
field6: int | bool | str | int  # PYI016: Duplicate union member `int`

# Shouldn't emit for non-type unions.
field7 = str | str

# Should emit for strangely-bracketed unions.
field8: int | (str | int)  # PYI016: Duplicate union member `int`

# Should handle user brackets when fixing.
field9: int | (int | str)  # PYI016: Duplicate union member `int`
field10: (str | int) | str  # PYI016: Duplicate union member `str`

# Should emit for nested unions.
field11: dict[int | int, str]

# Should emit for unions with more than two cases
field12: int | int | int  # Error
field13: int | int | int | int  # Error

# Should emit for unions with more than two cases, even if not directly adjacent
field14: int | int | str | int  # Error

# Should emit for duplicate literal types; also covered by PYI030
field15: typing.Literal[1] | typing.Literal[1]  # Error

# Shouldn't emit if in new parent type
field16: int | dict[int, str]  # OK

# Shouldn't emit if not in a union parent
field17: dict[int, int]  # OK

# Should emit in cases with newlines
field18: typing.Union[
    set[
        int  # foo
    ],
    set[
        int  # bar
    ],
]  # Error, newline and comment will not be emitted in message

# Should emit in cases with `typing.Union` instead of `|`
field19: typing.Union[int, int]  # Error

# Should emit in cases with nested `typing.Union`
field20: typing.Union[int, typing.Union[int, str]]  # Error

# Should emit in cases with mixed `typing.Union` and `|`
field21: typing.Union[int, int | str]  # Error

# Should emit only once in cases with multiple nested `typing.Union`
field22: typing.Union[int, typing.Union[int, typing.Union[int, int]]]  # Error

# Should emit in cases with newlines
field23: set[  # foo
    int] | set[int]

# Should emit twice (once for each `int` in the nested union, both of which are
# duplicates of the outer `int`), but not three times (which would indicate that
# we incorrectly re-checked the nested union).
field24: typing.Union[int, typing.Union[int, int]]  # PYI016: Duplicate union member `int`

# Should emit twice (once for each `int` in the nested union, both of which are
# duplicates of the outer `int`), but not three times (which would indicate that
# we incorrectly re-checked the nested union).
field25: typing.Union[int, int | int]  # PYI016: Duplicate union member `int`

# Should emit in cases with nested `typing.Union`
field26: typing.Union[typing.Union[int, int]]  # PYI016: Duplicate union member `int`

# Should emit in cases with nested `typing.Union`
field27: typing.Union[typing.Union[typing.Union[int, int]]]  # PYI016: Duplicate union member `int`

# Should emit in cases with mixed `typing.Union` and `|`
field28: typing.Union[int | int]  # Error

# Should emit twice in cases with multiple nested `typing.Union`
field29: typing.Union[int, typing.Union[typing.Union[int, int]]]  # Error

# Should emit once in cases with multiple nested `typing.Union`
field30: typing.Union[int, typing.Union[typing.Union[int, str]]]  # Error

# Should emit once, and fix to `typing.Union[float, int]`
field31: typing.Union[float, typing.Union[int | int]]  # Error

# Should emit once, and fix to `typing.Union[float, int]`
field32: typing.Union[float, typing.Union[int | int | int]]  # Error

# Test case for mixed union type fix
field33: typing.Union[typing.Union[int | int] | typing.Union[int | int]] # Error

# Test case for mixed union type
field34: typing.Union[list[int], str] | typing.Union[bytes, list[int]]  # Error

field35: "int | str | int"  # Error



# Technically, this falls into the domain of the rule but it is an unlikely edge case,
# only works if you have from `__future__ import annotations` at the top of the file,
# and stringified annotations are discouraged in stub files.
field36: "int | str" | int  # Ok

# https://github.com/astral-sh/ruff/issues/18546
# Expand Optional[T] to Union[T, None]
# OK
field37: typing.Optional[int]
field38: typing.Union[int, None]
# equivalent to None
field39: typing.Optional[None]
# equivalent to int | None
field40: typing.Union[typing.Optional[int], None]
field41: typing.Optional[typing.Union[int, None]]
field42: typing.Union[typing.Optional[int], typing.Optional[int]]
field43: typing.Optional[int] | None
field44: typing.Optional[int | None]
field45: typing.Optional[int] | typing.Optional[int]
# equivalent to int | dict | None
field46: typing.Union[typing.Optional[int], typing.Optional[dict]]
field47: typing.Optional[int] | typing.Optional[dict]

# avoid reporting twice
field48: typing.Union[typing.Optional[typing.Union[complex, complex]], complex]
field49: typing.Optional[complex | complex] | complex

# Regression test for https://github.com/astral-sh/ruff/issues/19403
# Should throw duplicate union member but not fix
isinstance(None, typing.Union[None, None])