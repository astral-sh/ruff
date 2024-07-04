#: Okay
True and False
#: E271
True and  False
#: E272
True  and False
#: E271
if   1:
    pass
#: E273
True and		False
#: E273 E274
True		and	False
#: E271
a and  b
#: E271
1 and  b
#: E271
a and  2
#: E271 E272
1  and  b
#: E271 E272
a  and  2
#: E272
this  and False
#: E273
a and	b
#: E274
a		and b
#: E273 E274
this		and	False
#: Okay
from u import (a, b)
from v import c, d
#: E271
from w import  (e, f)
#: E275
from w import(e, f)
#: E275
from importable.module import(e, f)
#: E275
try:
    from importable.module import(e, f)
except ImportError:
    pass
#: E275
if(foo):
    pass
else:
    pass
#: Okay
matched = {"true": True, "false": False}
#: E275:2:11
if True:
    assert(1)
#: Okay
def f():
    print((yield))
    x = (yield)
#: Okay
if (a and
    b):
    pass
#: Okay
def f():
	return 1

# Soft keywords

#: E271
type  Number = int

#: E273
type	Number = int

#: E275
match(foo):
    case(1):
        pass

# https://github.com/astral-sh/ruff/issues/12094
pass;

yield, x
