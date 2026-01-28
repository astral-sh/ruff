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


# Comments in annotation (unsafe fix)
def func7() -> U[
    None,
    # comment
    int
]:
    ...


# Nested unions - no fix should be provided
def func8(x: None | U[None, int]):
    ...


def func9(x: int | (str | None) | list):
    ...


def func10(x: U[int, U[None, list | set]]):
    ...


# Multiple annotations in the same function
def func11(x: None | int) -> None | int:
    ...


# With default argument (from poetry ecosystem check)
def func12(io: None | int = None) -> int | None:
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

