from typing import Union as U


def func1(arg: None | int): 
    ...


def func2() -> None | int: 
    ...


def func3(arg: None | None | int): 
    ...


def func4(arg: U[None, int]): 
    ...


def func5() -> U[None, int]: 
    ...


def func6(arg: U[None, None, int]): 
    ...


# Ok
def good_func1(arg: int | None): 
    ...


def good_func2() -> int | None: 
    ...


def good_func3(arg: None): 
    ...


def good_func4(arg: U[None]): 
    ...


def good_func5(arg: U[int]): 
    ...


# Autofix: should move None to the end
# ruff: fix
# expect: def auto1(arg: int | None): ...
def auto1(arg: None | int): ...

# Autofix: should move None to the end
# ruff: fix
# expect: def auto2() -> int | None: ...
def auto2() -> None | int: ...

# No autofix: multiple Nones
# ruff: fix
# expect: def nofix1(arg: None | None | int): ...
def nofix1(arg: None | None | int): ...

# No autofix: nested union
# ruff: fix
# expect: def nofix2(arg: U[int, U[None, list, set]]): ...
def nofix2(arg: U[int, U[None, list, set]]): ...

