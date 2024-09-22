# DOC402
def foo(num: int) -> str:
    """
    Do something

    Parameters
    ----------
    num : int
        A number
    """
    yield 'test'


# OK
def foo(num: int) -> str:
    """
    Do something

    Parameters
    ----------
    num : int
        A number

    Yields
    -------
    str
        A string
    """
    yield 'test'


class Bar:

    # OK
    def foo(self) -> str:
        """
        Do something

        Parameters
        ----------
        num : int
            A number

        Yields
        -------
        str
            A string
        """
        yield 'test'


    # DOC402
    def bar(self) -> str:
        """
        Do something

        Parameters
        ----------
        num : int
            A number
        """
        yield 'test'


import typing


# OK
def foo() -> typing.Generator[None, None, None]:
    """
    Do something
    """
    yield None


# OK
def foo() -> typing.Generator[None, None, None]:
    """
    Do something
    """
    yield


# DOC402
def foo() -> typing.Generator[int | None, None, None]:
    """
    Do something
    """
    yield None
    yield 1


# DOC402
def foo() -> typing.Generator[int, None, None]:
    """
    Do something
    """
    yield None


# OK
def foo():
    """
    Do something
    """
    yield None


# OK
def foo():
    """
    Do something
    """
    yield


# DOC402
def foo():
    """
    Do something
    """
    yield None
    yield 1


# DOC402
def foo():
    """
    Do something
    """
    yield 1
    yield


# DOC402
def bar() -> typing.Iterator[int | None]:
    """
    Do something
    """
    yield
