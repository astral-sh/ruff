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
