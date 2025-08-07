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

# Correct usage in loop and comprehension
def process_data():
    return 42
def test_correct_dummy_usage():
    my_list = [{"foo": 1}, {"foo": 2}]

    # Should NOT detect - dummy variable is not used
    [process_data() for _ in my_list]  # OK: `_` is ignored by rule

    # Should NOT detect - dummy variable is not used
    [item["foo"] for item in my_list]  # OK: not a dummy variable name

    # Should NOT detect - dummy variable is not used
    [42 for _unused in my_list]  # OK: `_unused` is not accessed


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


# Regular For Loops
def test_for_loops():
    my_list = [{"foo": 1}, {"foo": 2}]

    # Should detect used dummy variable
    for _item in my_list:
        print(_item["foo"])  # RUF052: Local dummy variable `_item` is accessed

    # Should detect used dummy variable
    for _index, _value in enumerate(my_list):
        result = _index + _value["foo"]  # RUF052: Both `_index` and `_value` are accessed

# List Comprehensions
def test_list_comprehensions():
    my_list = [{"foo": 1}, {"foo": 2}]

    # Should detect used dummy variable
    result = [_item["foo"] for _item in my_list]  # RUF052: Local dummy variable `_item` is accessed

    # Should detect used dummy variable in nested comprehension
    nested = [[_item["foo"] for _item in sublist] for _sublist in [my_list, my_list]]
    # RUF052: Both `_item` and `_sublist` are accessed

    # Should detect with conditions
    filtered = [_item["foo"] for _item in my_list if _item["foo"] > 0]
    # RUF052: Local dummy variable `_item` is accessed

# Dict Comprehensions
def test_dict_comprehensions():
    my_list = [{"key": "a", "value": 1}, {"key": "b", "value": 2}]

    # Should detect used dummy variable
    result = {_item["key"]: _item["value"] for _item in my_list}
    # RUF052: Local dummy variable `_item` is accessed

    # Should detect with enumerate
    indexed = {_index: _item["value"] for _index, _item in enumerate(my_list)}
    # RUF052: Both `_index` and `_item` are accessed

    # Should detect in nested dict comprehension
    nested = {_outer: {_inner["key"]: _inner["value"] for _inner in sublist}
              for _outer, sublist in enumerate([my_list])}
    # RUF052: `_outer`, `_inner` are accessed

# Set Comprehensions
def test_set_comprehensions():
    my_list = [{"foo": 1}, {"foo": 2}, {"foo": 1}]  # Note: duplicate values

    # Should detect used dummy variable
    unique_values = {_item["foo"] for _item in my_list}
    # RUF052: Local dummy variable `_item` is accessed

    # Should detect with conditions
    filtered_set = {_item["foo"] for _item in my_list if _item["foo"] > 0}
    # RUF052: Local dummy variable `_item` is accessed

    # Should detect with complex expression
    processed = {_item["foo"] * 2 for _item in my_list}
    # RUF052: Local dummy variable `_item` is accessed

# Generator Expressions
def test_generator_expressions():
    my_list = [{"foo": 1}, {"foo": 2}]

    # Should detect used dummy variable
    gen = (_item["foo"] for _item in my_list)
    # RUF052: Local dummy variable `_item` is accessed

    # Should detect when passed to function
    total = sum(_item["foo"] for _item in my_list)
    # RUF052: Local dummy variable `_item` is accessed

    # Should detect with multiple generators
    pairs = ((_x, _y) for _x in range(3) for _y in range(3) if _x != _y)
    # RUF052: Both `_x` and `_y` are accessed

    # Should detect in nested generator
    nested_gen = (sum(_inner["foo"] for _inner in sublist) for _sublist in [my_list] for sublist in _sublist)
    # RUF052: `_inner` and `_sublist` are accessed

# Complex Examples with Multiple Comprehension Types
def test_mixed_comprehensions():
    data = [{"items": [1, 2, 3]}, {"items": [4, 5, 6]}]

    # Should detect in mixed comprehensions
    result = [
        {_key: [_val * 2 for _val in _record["items"]] for _key in ["doubled"]}
        for _record in data
    ]
    # RUF052: `_key`, `_val`, and `_record` are all accessed

    # Should detect in generator passed to list constructor
    gen_list = list(_item["items"][0] for _item in data)
    # RUF052: Local dummy variable `_item` is accessed






