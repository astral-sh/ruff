"""Test: typing_extensions module imports."""
from foo import Literal

# F821 Undefined name `Model`
x: Literal["Model"]

from typing_extensions import Literal


# OK
x: Literal["Model"]


import typing_extensions


# OK
x: typing_extensions.Literal["Model"]
