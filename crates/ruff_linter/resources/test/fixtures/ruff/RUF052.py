##############
# Correct
##############

for _ in range(5):
    pass

_valid_type = int

_valid_var_1: _valid_type

_valid_var_1 = 1

_valid_var_2 = 2

_valid_var_3 = _valid_var_1 + _valid_var_2

def _valid_fun():
    pass

_valid_fun()

def fun(arg):
    _valid_unused_var = arg
    pass

_x = "global"

def fun():
    global _x
    return _x

def fun():
    __dunder__ = "dunder variable"
    return __dunder__

def fun():
    global _x
    _x = "reassigned global"
    return _x

# (we had a false positive when a global var is used like this in 2 functions)
_num: int

def set_num():
    global _num
    _num = 1

def print_num():
    print(_num)


class _ValidClass:
    pass

_ValidClass()

class ClassOk:
    _valid_private_cls_attr = 1

    print(_valid_private_cls_attr)

    def __init__(self):
        self._valid_private_ins_attr = 2
        print(self._valid_private_ins_attr)

    def _valid_method(self):
        return self._valid_private_ins_attr

    def method(arg):
        _valid_unused_var = arg
        return

def fun(x):
    _ = 1
    __ = 2
    ___ = 3
    if x == 1:
        return _
    if x == 2:
        return __
    if x == 3:
        return ___
    return x

###########
# Incorrect
###########

class Class_:
    def fun(self):
        _var = "method variable" # [RUF052]
        return _var

def fun(_var): # parameters are ignored
    return _var

def fun():
    _list = "built-in" # [RUF052]
    return _list

x = "global"

def fun():
    global x
    _x = "shadows global" # [RUF052]
    return _x

def foo():
  x = "outer"
  def bar():
    nonlocal x
    _x = "shadows nonlocal" # [RUF052]
    return _x
  bar()
  return x

def fun():
    x = "local"
    _x = "shadows local" # [RUF052]
    return _x


GLOBAL_1 = "global 1"
GLOBAL_1_ = "global 1 with trailing underscore"

def unfixables():
    _GLOBAL_1 = "foo"
    # unfixable because the rename would shadow a global variable
    print(_GLOBAL_1)  # [RUF052]

    local = "local"
    local_ = "local2"

    # unfixable because the rename would shadow a local variable
    _local = "local3"  # [RUF052]
    print(_local)

    def nested():
        _GLOBAL_1 = "foo"
        # unfixable because the rename would shadow a global variable
        print(_GLOBAL_1)  # [RUF052]

        # unfixable because the rename would shadow a variable from the outer function
        _local = "local4"
        print(_local)

def special_calls():
    from typing import TypeVar, ParamSpec, NamedTuple
    from enum import Enum
    from collections import namedtuple

    _P = ParamSpec("_P")
    _T = TypeVar(name="_T", covariant=True, bound=int|str)
    _NT = NamedTuple("_NT", [("foo", int)])
    _E = Enum("_E", ["a", "b", "c"])
    _NT2 = namedtuple("_NT2", ['x', 'y', 'z'])
    _NT3 = namedtuple(typename="_NT3", field_names=['x', 'y', 'z'])
    _DynamicClass = type("_DynamicClass", (), {})
    _NotADynamicClass = type("_NotADynamicClass")

    print(_T, _P, _NT, _E, _NT2, _NT3, _DynamicClass, _NotADynamicClass)

# Do not emit diagnostic if parameter is private
# even if it is later shadowed in the body of the function
# see https://github.com/astral-sh/ruff/issues/14968
class Node:

    connected: list[Node]

    def recurse(self, *, _seen: set[Node] | None = None):
        if _seen is None:
            _seen = set()
        elif self in _seen:
            return
        _seen.add(self)
        for other in self.connected:
            other.recurse(_seen=_seen)


def foo():
    _dummy_var = 42

    def bar():
        dummy_var = 43
        print(_dummy_var)


def foo():
    # Unfixable because both possible candidates for the new name are shadowed
    # in the scope of one of the references to the variable
    _dummy_var = 42

    def bar():
        dummy_var = 43
        dummy_var_ = 44
        print(_dummy_var)
