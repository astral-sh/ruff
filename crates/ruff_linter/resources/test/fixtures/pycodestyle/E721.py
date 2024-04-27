#: E721
if type(res) == type(42):
    pass
#: E721
if type(res) != type(""):
    pass
#: E721
if type(res) == memoryview:
    pass
#: Okay
import types

if res == types.IntType:
    pass
#: Okay
import types

if type(res) is not types.ListType:
    pass
#: E721
assert type(res) == type(False) or type(res) == type(None)
#: E721
assert type(res) == type([])
#: E721
assert type(res) == type(())
#: E721
assert type(res) == type((0,))
#: E721
assert type(res) == type((0))
#: E721
assert type(res) != type((1, ))
#: Okay
assert type(res) is type((1, ))
#: Okay
assert type(res) is not type((1, ))
#: E211 E721
assert type(res) == type ([2, ])
#: E201 E201 E202 E721
assert type(res) == type( ( ) )
#: E201 E202 E721
assert type(res) == type( (0, ) )
#:

#: Okay
import types

if isinstance(res, int):
    pass
if isinstance(res, str):
    pass
if isinstance(res, types.MethodType):
    pass
if isinstance(res, memoryview):
    pass
#: Okay
if type(res) is type:
    pass
#: E721
if type(res) == type:
    pass
#: Okay
def func_histype(a, b, c):
    pass
#: E722
try:
    pass
except:
    pass
#: E722
try:
    pass
except Exception:
    pass
except:
    pass
#: E722 E203 E271
try:
    pass
except  :
    pass
#: Okay
fake_code = """"
try:
    do_something()
except:
    pass
"""
try:
    pass
except Exception:
    pass
#: Okay
from . import custom_types as types

red = types.ColorTypeRED
red is types.ColorType.RED
#: Okay
from . import compute_type

if compute_type(foo) == 5:
    pass


class Foo:
    def asdf(self, value: str | None):
        #: E721
        if type(value) is str:
            ...


class Foo:
    def type(self):
        pass

    def asdf(self, value: str | None):
        #: E721
        if type(value) is str:
            ...


class Foo:
    def asdf(self, value: str | None):
        def type():
            pass

        # Okay
        if type(value) is str:
            ...


import numpy as np

#: Okay
x.dtype == float

#: Okay
np.dtype(int) == float

#: E721
dtype == float

import builtins

if builtins.type(res) == memoryview:  # E721
    pass
