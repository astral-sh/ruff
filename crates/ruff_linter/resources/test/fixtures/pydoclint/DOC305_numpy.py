
# DOC305
class Foo:
    """
    Docstring of the class

    Raises
    ------
        ValueError
    """

    def __init__(self, a: int, b: int) -> None:
        """
        Initialize class.

        Parameters
        ----------
        a : int
            Some integer.
        b : int
            Another integer.
        """
        self.a = a
        self.b = b
        raise ValueError


# OK
class Foo:
    """Docstring of the class"""

    def __init__(self, a: int, b: int) -> None:
        """
        Initialize class.

        Parameters
        ----------
        a : int
            Some integer.
        b : int
            Another integer.

        Raises
        ------
            ValueError
        """
        self.a = a
        self.b = b
        raise ValueError
