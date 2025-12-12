from typing import TypeAlias

T: TypeAlias = "T[0]"
def _(x: T):
    if x:
        pass
