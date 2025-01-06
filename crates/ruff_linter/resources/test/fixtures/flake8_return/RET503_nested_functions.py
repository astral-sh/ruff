from posix import abort
from typing import NoReturn

import typing_extensions



# function return type annotation NoReturn
def foo(x: int) -> int:
    def bar() -> NoReturn:
        abort()
    if x == 5:
        return 5
    bar()


def foo(string: str) -> str:
    def raises(value: str) -> NoReturn:
        raise RuntimeError("something went wrong")

    match string:
        case "a":
            return "first"
        case "b":
            return "second"
        case "c":
            return "third"
        case _:
            raises(string)


def foo() -> int:
    def baz() -> int:
        return 1


    def bar() -> NoReturn:
        a = 1 + 2
        raise AssertionError("Very bad")



    if baz() > 3:
        return 1
    bar()



# function return type annotation typing_extensions.Never
def foo(x: int) -> int:
    def bar() -> typing_extensions.Never:
        abort()
    if x == 5:
        return 5
    bar()


def foo(string: str) -> str:
    def raises(value: str) -> typing_extensions.Never:
        raise RuntimeError("something went wrong")

    match string:
        case "a":
            return "first"
        case "b":
            return "second"
        case "c":
            return "third"
        case _:
            raises(string)


def foo() -> int:
    def baz() -> int:
        return 1


    def bar() -> typing_extensions.Never:
        a = 1 + 2
        raise AssertionError("Very bad")



    if baz() > 3:
        return 1
    bar()
