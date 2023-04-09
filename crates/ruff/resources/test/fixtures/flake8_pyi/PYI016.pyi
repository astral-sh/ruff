# Shouldn't affect non-union field types
field1: str
# Should find duplicate field types
field2: str | str  # PYI016 Duplicate name in union

# Should affect union types in arguments
def func1(arg1: int | int):  # PYI016 Duplicate name in union
    # Should affect union expressions
    val = arg1 | arg1  # PYI016 Duplicate name in union
    print(arg1, val)

# Should affect in longer unions
field3: str | str | int  # PYI016 Duplicate name in union
field4: int | int | str  # PYI016 Duplicate name in union
field5: str | int | str  # PYI016 Duplicate name in union
field5: int | bool | str | int  # PYI016 Duplicate name in union

# More complex tests

field6 = 1 | 0 | 1  # PYI016 Duplicate literal in union
field7 = 0x1 | 0x4 | 0x1 | 0x4  # PYI016 Duplicate literal in union (x2)
field8 = (
    "abc" | float | "abc" | float
)  # PYI016 Duplicate literal in union, PYI016 Duplicate name in union
field11 = 0 | 0x0  # PYI016 Duplicate literal in union

# Confusing, but valid statements
field9 = "None" | None
field10 = 0 | "0" | False
