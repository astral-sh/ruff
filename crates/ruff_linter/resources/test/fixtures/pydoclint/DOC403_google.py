# OK
def foo(num: int) -> str:
    """
    Do something

    Args:
        num (int): A number
    """
    print('test')


# DOC403
def foo(num: int) -> str:
    """
    Do something

    Args:
        num (int): A number

    Yields:
        str: A string
    """
    print('test')


class Bar:

    # DOC403
    def foo(self) -> str:
        """
        Do something

        Args:
            num (int): A number

        Yields:
            str: A string
        """
        print('test')


    # OK
    def bar(self) -> str:
        """
        Do something

        Args:
            num (int): A number
        """
        print('test')


import typing


# OK
def foo() -> typing.Generator[None, None, None]:
    """
    Do something

    Yields:
        When X.
    """
    yield


# OK
def foo() -> typing.Generator[None, None, None]:
    """
    Do something

    Yields:
        When X.
    """
    yield None


# OK
def foo():
    """
    Do something

    Yields:
        When X.
    """
    yield


# OK
def foo():
    """
    Do something

    Yields:
        When X.
    """
    yield None
