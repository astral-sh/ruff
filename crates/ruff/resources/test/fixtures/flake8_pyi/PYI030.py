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
