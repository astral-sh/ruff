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


# OK - implicit None early return
def foo(obj: object) -> None:
    """A very helpful docstring.

    Parameters
    ----------
    obj : object
        An object.
    """
    if obj is None:
        return
    print(obj)


# OK - explicit None early return
def foo(obj: object) -> None:
    """A very helpful docstring.

    Parameters
    ----------
    obj : object
        An object.
    """
    if obj is None:
        return None
    print(obj)


# OK - explicit None early return w/o useful type annotations
def foo(obj):
    """A very helpful docstring.

    Parameters
    ----------
    obj : object
        An object.
    """
    if obj is None:
        return None
    print(obj)


# OK - multiple explicit None early returns
def foo(obj: object) -> None:
    """A very helpful docstring.

    Parameters
    ----------
    obj : object
        An object.
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

    Parameters
    ----------
    x : int
        An integer.
    """
    if x < 0:
        return None
    else:
        return x


# DOC201 - non-early return explicit None w/o useful type annotations
def foo(x):
    """A very helpful docstring.

    Parameters
    ----------
    x : int
        An integer.
    """
    if x < 0:
        return None
    else:
        return x


# DOC201 - only returns None, but return annotation is not None
def foo(s: str) -> str | None:
    """A very helpful docstring.

    Parameters
    ----------
    x : str
        A string.
    """
    return None


# DOC201
def bar() -> int | None:
    """Bar-y method"""
    return


from collections.abc import Iterator, Generator


# This is okay -- returning `None` is implied by `Iterator[str]`;
# no need to document it
def generator_function() -> Iterator[str]:
    """Generate some strings"""
    yield from "abc"
    return


# This is okay -- returning `None` is stated by `Generator[str, None, None]`;
# no need to document it
def generator_function_2() -> Generator[str, None, None]:
    """Generate some strings"""
    yield from "abc"
    return


# DOC201 -- returns None but `Generator[str, None, int | None]`
# indicates it could sometimes return `int`
def generator_function_3() -> Generator[str, None, int | None]:
    """Generate some strings"""
    yield from "abc"
    return


# DOC201 -- no type annotation and a non-None return
# indicates it could sometimes return `int`
def generator_function_4():
    """Generate some strings"""
    yield from "abc"
    return 42


# DOC201 -- no `yield` expressions, so not a generator function
def not_a_generator() -> Iterator[int]:
    """"No returns documented here, oh no"""
    return (x for x in range(42))


# DOC201 - OK (generator with only bare return; Iterable annotation is not Iterator/Generator)
# Regression test for https://github.com/astral-sh/ruff/issues/21336
def generator_bare_return(x: int) -> Iterable[int]:
    """Yield integers up to x.

    Parameters
    ----------
    x : int
        Upper bound.
    """
    for i in range(x):
        yield i
    return


# DOC201 - OK (generator with explicit return None; same annotation)
def generator_explicit_none(x: int) -> Iterable[int]:
    """Yield integers up to x.

    Parameters
    ----------
    x : int
        Upper bound.
    """
    for i in range(x):
        yield i
    return None
