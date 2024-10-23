# DOC402
def foo(num: int) -> str:
    """
    Do something

    Args:
        num (int): A number
    """
    yield 'test'


# OK
def foo(num: int) -> str:
    """
    Do something

    Args:
        num (int): A number

    Yields:
        str: A string
    """
    yield 'test'


class Bar:

    # OK
    def foo(self) -> str:
        """
        Do something

        Args:
            num (int): A number

        Yields:
            str: A string
        """
        yield 'test'


    # DOC402
    def bar(self) -> str:
        """
        Do something

        Args:
            num (int): A number
        """
        yield 'test'


# OK
def test():
    """Do something."""
    # DOC402
    def nested():
        """Do something nested."""
        yield 5

    print("I never yield")


# DOC402
def test():
    """Do something."""
    yield from range(10)


# OK
def f():
    """Yields 1."""
    yield 1


# OK
def f():
    """Yield 1."""
    yield 1


# OK
def f(num: int):
    """Yields 1.

    Args:
        num (int): A number
    """
    yield 1


import collections.abc


# DOC402
def foo() -> collections.abc.Generator[int | None, None, None]:
    """
    Do something
    """
    yield


# DOC402
def bar() -> collections.abc.Iterator[int | None]:
    """
    Do something
    """
    yield
