from typing import Literal


def func1(arg1: Literal[None]): ...


def func2(arg1: Literal[None] | int): ...


def func3() -> Literal[None]: ...


def func4(arg1: Literal[int, None, float]): ...


def func5(arg1: Literal[None, None]): ...


def func6(arg1: Literal[
    "hello",
    None  # Comment 1
    , "world"
    ]): ...


def func7(arg1: Literal[
    None  # Comment 1
    ]): ...


# OK
def good_func(arg1: Literal[int] | None): ...


# From flake8-pyi
Literal[None]  # Y061 None inside "Literal[]" expression. Replace with "None"
Literal[True, None]  # Y061 None inside "Literal[]" expression. Replace with "Literal[True] | None"

###
# The following rules here are slightly subtle,
# but make sense when it comes to giving the best suggestions to users of flake8-pyi.
###

# If Y061 and Y062 both apply, but all the duplicate members are None,
# only emit Y061...
Literal[None, None]  # Y061 None inside "Literal[]" expression. Replace with "None"
Literal[1, None, "foo", None]  # Y061 None inside "Literal[]" expression. Replace with "Literal[1, 'foo'] | None"

# ... but if Y061 and Y062 both apply
# and there are no None members in the Literal[] slice,
# only emit Y062:
Literal[None, True, None, True]  # Y062 Duplicate "Literal[]" member "True"
