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
