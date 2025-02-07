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

def f(x, *args, **kwargs):
    """Do something.

    Args:
        x: the value
        *args: var-arguments
    """
    return x


# regression test for https://github.com/astral-sh/ruff/issues/16007.
# attributes is a section name without subsections, so it was failing the
# previous workaround for Args: args: sections
def send(payload: str, attributes: dict[str, Any]) -> None:
    """
    Send a message.

    Args:
        payload:
            The message payload.

        attributes:
            Additional attributes to be sent alongside the message.
    """


# undocumented argument with the same name as a section
def should_fail(payload, Args):
    """
    Send a message.

    Args:
        payload:
            The message payload.
    """


# documented argument with the same name as a section
def should_not_fail(payload, Args):
    """
    Send a message.

    Args:
        payload:
            The message payload.

        Args:
            The other arguments.
    """
