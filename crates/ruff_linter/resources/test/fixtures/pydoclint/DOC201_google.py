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


import abc


class A(metaclass=abc.abcmeta):
    # DOC201
    @abc.abstractmethod
    def f(self):
        """Lorem ipsum."""
        return True


# OK - implicit None early return
def foo(obj: object) -> None:
    """A very helpful docstring.

    Args:
        obj (object): An object.
    """
    if obj is None:
        return
    print(obj)


# OK - explicit None early return
def foo(obj: object) -> None:
    """A very helpful docstring.

    Args:
        obj (object): An object.
    """
    if obj is None:
        return None
    print(obj)


# OK - explicit None early return w/o useful type annotations
def foo(obj):
    """A very helpful docstring.

    Args:
        obj (object): An object.
    """
    if obj is None:
        return None
    print(obj)


# OK - multiple explicit None early returns
def foo(obj: object) -> None:
    """A very helpful docstring.

    Args:
        obj (object): An object.
    """
    if obj is None:
        return None
    if obj == "None":
        return
    if obj == 0:
        return None
    print(obj)


# DOC201 - non-early return explicit None
def foo(x: int) -> int | None:
    """A very helpful docstring.

    Args:
        x (int): An integer.
    """
    if x < 0:
        return None
    else:
        return x


# DOC201 - non-early return explicit None w/o useful type annotations
def foo(x):
    """A very helpful docstring.

    Args:
        x (int): An integer.
    """
    if x < 0:
        return None
    else:
        return x


# DOC201 - only returns None, but return annotation is not None
def foo(s: str) -> str | None:
    """A very helpful docstring.

    Args:
        s (str): A string.
    """
    return None


class Spam:
    # OK
    def __new__(cls) -> 'Spam':
        """New!!"""
        return cls()
