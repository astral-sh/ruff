from pydantic import BaseModel
from dataclasses import dataclass

# Class fields (AnnAssign) should now trigger PYI041
class Foo(BaseModel):
    bar: int | float  # ERROR: PYI041

@dataclass
class Baz:
    quux: int | float  # ERROR: PYI041

# Standard variable annotations (AnnAssign)
variable: int | float = 1.0  # ERROR: PYI041

# Function parameters (Parameter) should still work
def func(_quux: int | float):  # ERROR: PYI041
    pass