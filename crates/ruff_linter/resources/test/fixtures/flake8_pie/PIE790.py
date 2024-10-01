class Foo:
    """buzz"""

    pass


if foo:
    """foo"""
    pass


def multi_statement() -> None:
    """This is a function."""
    pass; print("hello")


if foo:
    pass
else:
    """bar"""
    pass


while True:
    pass
else:
    """bar"""
    pass


for _ in range(10):
    pass
else:
    """bar"""
    pass


async for _ in range(10):
    pass
else:
    """bar"""
    pass


def foo() -> None:
    """
    buzz
    """

    pass


async def foo():
    """
    buzz
    """

    pass


try:
    """
    buzz
    """
    pass
except ValueError:
    pass


try:
    bar()
except ValueError:
    """bar"""
    pass


for _ in range(10):
    """buzz"""
    pass

async for _ in range(10):
    """buzz"""
    pass

while cond:
    """buzz"""
    pass


with bar:
    """buzz"""
    pass

async with bar:
    """buzz"""
    pass


def foo() -> None:
    """buzz"""
    pass  # bar


class Foo:
    # bar
    pass


if foo:
    # foo
    pass


class Error(Exception):
    pass


try:
    foo()
except NetworkError:
    pass


def foo() -> None:
    pass


def foo():
    print("foo")
    pass


def foo():
    """A docstring."""
    print("foo")
    pass


for i in range(10):
    pass
    pass

for i in range(10):
    pass

    pass

for i in range(10):
    pass  # comment
    pass


def foo():
    print("foo")
    ...


def foo():
    """A docstring."""
    print("foo")
    ...


for i in range(10):
    ...
    ...

for i in range(10):
    ...

    ...

for i in range(10):
    ...  # comment
    ...

for i in range(10):
    ...
    pass

from typing import Protocol


class Repro(Protocol):
    def func(self) -> str:
        """Docstring"""
        ...

    def impl(self) -> str:
        """Docstring"""
        return self.func()


import abc


class Repro:
    @abc.abstractmethod
    def func(self) -> str:
        """Docstring"""
        ...

    def impl(self) -> str:
        """Docstring"""
        return self.func()

    def stub(self) -> str:
        """Docstring"""
        ...


class Repro(Protocol[int]):
    def func(self) -> str:
        """Docstring"""
        ...

    def impl(self) -> str:
        """Docstring"""
        return self.func()


class Repro[int](Protocol):
    def func(self) -> str:
        """Docstring"""
        ...

    def impl(self) -> str:
        """Docstring"""
        return self.func()


import typing

if typing.TYPE_CHECKING:
    def contains_meaningful_ellipsis() -> list[int]:
        """Allow this in a TYPE_CHECKING block."""
        ...
