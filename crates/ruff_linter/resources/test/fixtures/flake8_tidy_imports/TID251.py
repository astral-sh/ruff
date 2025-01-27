## Banned modules ##
import cgi

from cgi import *

from cgi import a, b, c

# banning a module also bans any submodules
import cgi.foo.bar

from cgi.foo import bar

from cgi.foo.bar import *

## Banned module members ##

from typing import TypedDict

import typing

# attribute access is checked
typing.TypedDict

typing.TypedDict.anything

# function calls are checked
typing.TypedDict()

typing.TypedDict.anything()

# import aliases are resolved
import typing as totally_not_typing
totally_not_typing.TypedDict

# relative imports are respected
from .typing import TypedDict


# https://github.com/astral-sh/ruff/issues/10079
list()

from builtins import list as l
l()

import builtins
builtins.list

class C:
    @property
    def foo(self): ...
