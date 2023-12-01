from typing import NoReturn, Never
import typing_extensions


def foo(arg):
    ...


def foo_int(arg: int):
    ...


def foo_no_return(arg: NoReturn):
    ...


def foo_no_return_typing_extensions(
    arg: typing_extensions.NoReturn,
):
    ...


def foo_no_return_kwarg(arg: int, *, arg2: NoReturn):
    ...


def foo_no_return_pos_only(arg: int, /, arg2: NoReturn):
    ...


def foo_never(arg: Never):
    ...
