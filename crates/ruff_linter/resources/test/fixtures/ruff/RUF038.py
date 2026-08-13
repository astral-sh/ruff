from typing import Literal, get_args


def func1(arg1: Literal[True, False]):
    ...


def func2(arg1: Literal[True, False, True]): 
    ...


def func3() -> Literal[True, False]: 
    ...


def func4(arg1: Literal[True, False] | bool): 
    ...


def func5(arg1: Literal[False, True]):
    ...


def func6(arg1: Literal[True, False, "hello", "world"]):
    ...

# ok
def good_func1(arg1: bool):
    ...


def good_func2(arg1: Literal[True]):
    ...


# ok: runtime `Literal` is a real value; rewriting to `bool` would change what
# `get_args` returns, so this must not be flagged.
get_args(Literal[True, False])


values = ["sentinel"]


# A non-`Literal` member (here a subscript) must not be dropped: the rule may flag the
# redundancy but must not offer a fix that collapses to `bool` and discards `values[0]`.
def func7(arg1: Literal[True, False, values[0]]):
    ...
