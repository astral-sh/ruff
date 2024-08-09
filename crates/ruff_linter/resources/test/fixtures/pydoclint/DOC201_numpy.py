# DOC201
def foo(num: int) -> str:
    """
    Do something

    Parameters
    ----------
    num : int
        A number
    """
    return 'test'


# OK
def foo(num: int) -> str:
    """
    Do something

    Parameters
    ----------
    num : int
        A number

    Returns
    -------
    str
        A string
    """
    return 'test'


class Bar:

    # OK
    def foo(self) -> str:
        """
        Do something

        Parameters
        ----------
        num : int
            A number

        Returns
        -------
        str
            A string
        """
        return 'test'


    # DOC201
    def bar(self) -> str:
        """
        Do something

        Parameters
        ----------
        num : int
            A number
        """
        return 'test'


    # OK
    @property
    def baz(self) -> str:
        """
        Do something

        Parameters
        ----------
        num : int
            A number
        """
        return 'test'


import abc


class A(metaclass=abc.abcmeta):
    # DOC201
    @abc.abstractmethod
    def f(self):
        """Lorem ipsum."""
        return True
