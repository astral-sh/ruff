def f(x, y, z):
    """Do something.

    Args:
        x: the value
            with a hanging indent

    Returns:
        the value
    """
    return x


def f(x, y, z):
    """Do something.

    Args:
        x:
            The whole thing has a hanging indent.

    Returns:
        the value
    """
    return x


def f(x, y, z):
    """Do something.

    Args:
        x:
            The whole thing has a hanging indent.

    Returns: the value
    """
    return x


def f(x, y, z):
    """Do something.

    Args:
        x: the value def
            ghi

    Returns:
        the value
    """
    return x


def f(x, y, z):
    """Do something.

    Args:
        x: the value
        z: A final argument

    Returns:
        the value
    """
    return x


def f(x, y, z):
    """Do something.

    Args:
        x: the value
        z: A final argument

    Returns: the value
    """
    return x


def f(x, y, z):
    """Do something.

    Args:
        x: the value
        z: A final argument
    """
    return x


def f(x, *args, **kwargs):
    """Do something.

    Args:
        x: the value
        *args: variable arguments
        **kwargs: keyword arguments
    """
    return x


def f(x, *args, **kwargs):
    """Do something.

    Args:
        *args: variable arguments
        **kwargs: keyword arguments
    """
    return x


def f(x, *args, **kwargs):
    """Do something.

    Args:
        x: the value
        **kwargs: keyword arguments
    """
    return x


def f(x, *, y, z):
    """Do something.

    Args:
        x: some first value

    Keyword Args:
        y (int): the other value
        z (int): the last value

    """
    return x, y, z

def f(x):
    """Do something with valid description.

    Args:
    ----
        x: the value

    Returns:
    -------
        the value
    """
    return x


class Test:
    def f(self, /, arg1: int) -> None:
        """
        Some beauty description.

        Args:
            arg1: some description of arg
        """


def select_data(
    query: str,
    args: tuple,
    database: str,
    auto_save: bool,
) -> None:
    """This function has an argument `args`, which shouldn't be mistaken for a section.

    Args:
        query:
            Query template.
        args:
            A list of arguments.
        database:
            Which database to connect to ("origin" or "destination").
    """
