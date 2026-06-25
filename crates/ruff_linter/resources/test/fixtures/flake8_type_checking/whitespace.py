# Regression test for: https://github.com/astral-sh/ruff/issues/19175
# there is a (potentially invisible) unicode formfeed character (000C) between `TYPE_CHECKING` and the backslash
from typing import TYPE_CHECKING\

if TYPE_CHECKING: import builtins
builtins.print("!")
