import io
from pathlib import Path


async def foo():
    open("")  # ASYNC230
    io.open_code("")  # ASYNC230

    with open(""):  # ASYNC230
        ...

    with open("") as f:  # ASYNC230
        ...

    with foo(), open(""):  # ASYNC230
        ...

    async with open(""):  # ASYNC230
        ...


def foo_sync():
    open("")

# Violation cases:


async def func():
    open("foo")  # ASYNC230


# Violation cases for pathlib:


async def func():
    Path("foo").open()  # ASYNC230


async def func():
    p = Path("foo")
    p.open()  # ASYNC230


async def func():
    with Path("foo").open() as f:  # ASYNC230
        pass


async def func() -> None:
    p = Path("foo")

    async def bar():
        p.open()  # ASYNC230


async def func() -> None:
    (p1, p2) = (Path("foo"), Path("bar"))

    p1.open()  # ASYNC230


# Non-violation cases for pathlib:


class Foo:
    def open(self):
        pass


async def func():
    Foo().open()  # OK


async def func():
    def open():
        pass

    open()  # OK


def func():
    Path("foo").open()  # OK
