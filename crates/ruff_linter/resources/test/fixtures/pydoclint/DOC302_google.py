
# DOC302
class Foo:
    """
    Docstring of the class

    Returns:
        int: Final integer.
    """

    def __init__(self, a: int, b: int) -> None:
        """
        Initialize class.

        Args:
            a (int): Some integer.
            b (int): Another integer.
        """
        self.a = a
        self.b = b


# OK
class Foo:
    """
    Docstring of the class
    """

    def __init__(self, a: int, b: int) -> None:
        """
        Initialize class.

        Args:
            a (int): Some integer.
            b (int): Another integer.
        """
        self.a = a
        self.b = b
