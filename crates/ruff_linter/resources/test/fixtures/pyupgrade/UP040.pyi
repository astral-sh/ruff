import typing
from typing import TypeAlias

# UP040
# Fixes in type stub files should be safe to apply unlike in regular code where runtime behavior could change
x: typing.TypeAlias = int
x: TypeAlias = int
