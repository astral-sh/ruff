"""Test: typing module imports."""
from foo import cast

# OK
x = cast("Model")

from typing import cast


# F821 Undefined name `Model`
x = cast("Model", x)


import typing


# F821 Undefined name `Model`
x = typing.cast("Model", x)


from typing import Pattern

# F821 Undefined name `Model`
x = Pattern["Model"]


from typing.re import Match

# F821 Undefined name `Model`
x = Match["Model"]
