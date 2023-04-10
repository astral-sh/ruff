# Shouldn't affect non-union field types
field1: str

# Should emit for duplicate field types
field2: str | str  # PYI016 Duplicate name in union

# Should emit for union types in arguments
def func1(arg1: int | int):  # PYI016 Duplicate name in union
    print(arg1)

# Should emit for unions in return types
def func2() -> str | str:  # PYI016 Duplicate name in union
    return "my string"

# Should emit in longer unions, even if not directly adjacent
field3: str | str | int  # PYI016 Duplicate name in union
field4: int | int | str  # PYI016 Duplicate name in union
field5: str | int | str  # PYI016 Duplicate name in union
field6: int | bool | str | int  # PYI016 Duplicate name in union

# Shouldn't emit for non-type unions
field7 = str | str
