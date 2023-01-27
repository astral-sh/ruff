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
