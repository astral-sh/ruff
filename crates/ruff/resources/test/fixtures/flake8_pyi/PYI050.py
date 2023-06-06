from typing import NoReturn, Never


def foo(arg):
    ...


def foo_int(arg: int):
    ...


def foo_no_return(arg: NoReturn):
    ...  # Error: PYI050


def foo_no_return_kwarg(arg: int, *, arg2: NoReturn):
    ...  # Error: PYI050


def foo_no_return_pos_only(arg: int, /, arg2: NoReturn):
    ...  # Error: PYI050


def foo_never(arg: Never):
    ...
