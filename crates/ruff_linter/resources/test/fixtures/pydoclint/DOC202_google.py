# OK
def foo(num: int) -> str:
    """
    Do something

    Args:
        num (int): A number
    """
    print('test')


# DOC202
def foo(num: int) -> str:
    """
    Do something

    Args:
        num (int): A number

    Returns:
        str: A string
    """
    print('test')


class Bar:

    # DOC202
    def foo(self) -> str:
        """
        Do something

        Args:
            num (int): A number

        Returns:
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


# See: https://github.com/astral-sh/ruff/issues/12650
class C:
    def foo(self) -> int:
        """Calculate x.

        Returns:
            x
        """
        raise NotImplementedError


import abc


class A(metaclass=abc.abcmeta):
    @abc.abstractmethod
    def f(self):
        """Lorem ipsum

        Returns:
            dict: The values
        """
        return


# DOC202 -- never explicitly returns anything, just short-circuits
def foo(s: str, condition: bool):
    """Fooey things.

    Returns:
        None
    """
    if not condition:
        return
    print(s)
