# DOC402
def foo(num: int) -> str:
    """
    Do something

    Args:
        num (int): A number
    """
    yield 'test'


# OK
def foo(num: int) -> str:
    """
    Do something

    Args:
        num (int): A number

    Yields:
        str: A string
    """
    yield 'test'


class Bar:

    # OK
    def foo(self) -> str:
        """
        Do something

        Args:
            num (int): A number

        Yields:
            str: A string
        """
        yield 'test'


    # DOC402
    def bar(self) -> str:
        """
        Do something

        Args:
            num (int): A number
        """
        yield 'test'


# OK
def test():
    """Do something."""
    # DOC402
    def nested():
        """Do something nested."""
        yield 5

    print("I never yield")


# DOC402
def test():
    """Do something."""
    yield from range(10)

