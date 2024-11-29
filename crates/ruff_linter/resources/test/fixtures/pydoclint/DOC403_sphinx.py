# OK
def foo(num: int) -> str:
    """
    Do something

    :param num: A number
    :type num: int
    """
    print('test')


# DOC403
def foo(num: int) -> str:
    """
    Do something

    :param num: A number
    :type num: int
    :yield: A string
    :rtype: str
    """
    print('test')


class Bar:

    # DOC403
    def foo(self) -> str:
        """
        Do something

        :param num: A number
        :type num: int
        :yield: A string
        :rtype: str
        """
        print('test')


    # OK
    def bar(self) -> str:
        """
        Do something

        :param num: A number
        :type num: int
        """
        print('test')


import typing


# OK
def foo() -> typing.Generator[None, None, None]:
    """
    Do something

    :yield: When X.
    """
    yield None


# OK
def foo() -> typing.Generator[None, None, None]:
    """
    Do something

    :yield: When X.
    """
    yield


# OK
def foo():
    """
    Do something

    :yield: When X.
    """
    yield None


# OK
def foo():
    """
    Do something

    :yield: When X.
    """
    yield
