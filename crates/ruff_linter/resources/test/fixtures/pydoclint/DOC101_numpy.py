# DOC101
def foo(num: int, name: str) -> str:
    """
    Do something

    Parameters
    ----------
    num : int
        A number
    """
    return 'test'


# OK
def foo(num: int, name: str) -> str:
    """
    Do something

    Parameters
    ----------
    num : int
        A number
    name : str
        A name string
    """
    return 'test'


class Bar:
    # DOC101
    def foo(self, num: int, name: str) -> str:
        """
        Do something

        Parameters
        ----------
        num : int
            A number
        """
        return 'test'

    # OK
    def bar(self, num: int, name: str) -> str:
        """
        Do something

        Parameters
        ----------
        num : int
            A number
        name : str
            A name string
        """
        return 'test'

    # OK - properties don't need parameter docs
    @property
    def baz(self) -> str:
        """Do something."""
        return 'test'


# OK - nested functions don't need parameter docs
def test():
    """Do something."""
    def nested(x: int):
        """Do something nested."""
        return x
    return nested(5)


from functools import cached_property

class Baz:
    # OK - cached properties don't need parameter docs
    @cached_property
    def baz(self) -> str:
        """Do something."""
        return 'test'


import abc

class Abstract(metaclass=abc.ABCMeta):
    # DOC101
    @abc.abstractmethod
    def method(self, x: int, y: int):
        """
        Parameters
        ----------
        x : int
            First number
        """
        pass


# Special cases for *args and **kwargs
# DOC101
def with_args(*args, **kwargs):
    """
    Parameters
    ----------
    args : tuple
        Positional arguments
    """
    pass


# OK
def with_args(*args, **kwargs):
    """
    Parameters
    ----------
    args : tuple
        Positional arguments
    kwargs : dict
        Keyword arguments
    """
    pass
