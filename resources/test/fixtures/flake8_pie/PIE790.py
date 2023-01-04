class Foo:
    """buzz"""

    pass  # PIE790


if foo:
    """foo"""
    pass  # PIE790


def multi_statement() -> None:
    """This is a function."""
    pass; print("hello")


if foo:
    pass
else:
    """bar"""
    pass  # PIE790


while True:
    pass
else:
    """bar"""
    pass  # PIE790


for _ in range(10):
    pass
else:
    """bar"""
    pass  # PIE790


async for _ in range(10):
    pass
else:
    """bar"""
    pass  # PIE790


def foo() -> None:
    """
    buzz
    """

    pass  # PIE790


async def foo():
    """
    buzz
    """

    pass  # PIE790


try:
    """
    buzz
    """
    pass  # PIE790
except ValueError:
    pass


try:
    bar()
except ValueError:
    """bar"""
    pass  # PIE790


for _ in range(10):
    """buzz"""
    pass  # PIE790

async for _ in range(10):
    """buzz"""
    pass  # PIE790

while cond:
    """buzz"""
    pass  # PIE790


with bar:
    """buzz"""
    pass  # PIE790

async with bar:
    """buzz"""
    pass  # PIE790


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
