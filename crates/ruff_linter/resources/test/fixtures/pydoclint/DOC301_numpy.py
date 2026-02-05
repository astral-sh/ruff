
class Foo:
    """Docstring of the class"""

    # DOC301
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


class Foo:
    """Docstring of the class"""

    # OK
    def __init__(self, a: int, b: int) -> None:
        self.a = a
        self.b = b
