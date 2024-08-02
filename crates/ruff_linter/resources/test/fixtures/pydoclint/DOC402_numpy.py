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
