"""Test: no-blank-line-after-function special-cases around comment handling."""

# OK
def outer():
    """This is a docstring."""
    def inner():
        return

    return inner


# OK
def outer():
    """This is a docstring."""

    def inner():
        return

    return inner


# OK
def outer():
    """This is a docstring."""
    # This is a comment.
    def inner():
        return

    return inner


# OK
def outer():
    """This is a docstring."""

    # This is a comment.
    def inner():
        return

    return inner


# OK
def outer():
    """This is a docstring."""

    # This is a comment.
    # Followed by another comment.
    def inner():
        return

    return inner


# D202
def outer():
    """This is a docstring."""


    def inner():
        return

    return inner


# D202
def outer():
    """This is a docstring."""


    # This is a comment.
    def inner():
        return

    return inner


# D202
def outer():
    """This is a docstring."""

    # This is a comment.

    def inner():
        return

    return inner
