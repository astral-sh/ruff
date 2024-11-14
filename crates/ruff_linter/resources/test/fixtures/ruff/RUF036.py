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

