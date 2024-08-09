# DOC201
def foo(num: int) -> str:
    """
    Do something

    Args:
        num (int): A number
    """
    return 'test'


# OK
def foo(num: int) -> str:
    """
    Do something

    Args:
        num (int): A number

    Returns:
        str: A string
    """
    return 'test'


class Bar:

    # OK
    def foo(self) -> str:
        """
        Do something

        Args:
            num (int): A number

        Returns:
            str: A string
        """
        return 'test'


    # DOC201
    def bar(self) -> str:
        """
        Do something

        Args:
            num (int): A number
        """
        return 'test'


    # OK
    @property
    def baz(self) -> str:
        """
        Do something

        Args:
            num (int): A number
        """
        return 'test'


# OK
def test():
    """Do something."""
    # DOC201
    def nested():
        """Do something nested."""
        return 5

    print("I never return")


from functools import cached_property

class Baz:
    # OK
    @cached_property
    def baz(self) -> str:
        """
        Do something

        Args:
            num (int): A number
        """
        return 'test'


# OK
def f():
    """Returns 1."""
    return 1


# OK
def f():
    """Return 1."""
    return 1


# OK
def f(num: int):
    """Returns 1.

    Args:
        num (int): A number
    """
    return 1
