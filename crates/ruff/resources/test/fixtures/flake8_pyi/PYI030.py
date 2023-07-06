from typing import Literal
# Shouldn't emit for any cases in the non-stub file for compatibility with flake8-pyi.
# Note that this rule could be applied here in the future.

field1: Literal[1]  # OK
field2: Literal[1] | Literal[2]  # OK

def func1(arg1: Literal[1] | Literal[2]):  # OK
    print(arg1)


def func2() -> Literal[1] | Literal[2]:  # OK
    return "my Literal[1]ing"


field3: Literal[1] | Literal[2] | str  # OK
field4: str | Literal[1] | Literal[2]  # OK
field5: Literal[1] | str | Literal[2]  # OK
field6: Literal[1] | bool | Literal[2] | str  # OK
field7 = Literal[1] | Literal[2]  # OK
field8: Literal[1] | (Literal[2] | str)  # OK
field9: Literal[1] | (Literal[2] | str)  # OK
field10: (Literal[1] | str) | Literal[2]  # OK
field11: dict[Literal[1] | Literal[2], str]  # OK
