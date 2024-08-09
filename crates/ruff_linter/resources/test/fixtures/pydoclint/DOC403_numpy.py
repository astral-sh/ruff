# OK
def foo(num: int) -> str:
    """
    Do something

    Parameters
    ----------
    num : int
        A number
    """
    print('test')


# DOC403
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
    print('test')


class Bar:

    # DOC403
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
        print('test')


    # OK
    def bar(self) -> str:
        """
        Do something

        Parameters
        ----------
        num : int
            A number
        """
        print('test')
