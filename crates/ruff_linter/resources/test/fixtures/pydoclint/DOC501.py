# https://github.com/astral-sh/ruff/issues/12428
def parse_bool(x, default=_parse_bool_sentinel):
    """Parse a boolean value
    bool or type(default)
    Raises
    `ValueError`
   ê>>> all(parse_bool(x) for x in [True, "yes", "Yes", "true", "True", "on", "ON", "1", 1])
    """


# https://github.com/astral-sh/ruff/issues/12647
def get_bar(self) -> str:
    """Print and return bar.

    Raises:
        ValueError: bar is not bar.

    Returns:
        str: bar value.
    """


# https://github.com/astral-sh/ruff/issues/19219
from . import RelativeException

def test_relative_import():
    """Function that raises a relatively imported exception."""
    raise RelativeException


from . import NotImplementedError

def test_imported_not_implemented_error():
    """Function that raises imported NotImplementedError (should not trigger DOC501)."""
    raise NotImplementedError


def test_builtin_not_implemented_error():
    """Function that raises builtin NotImplementedError (should not trigger DOC501)."""
    raise NotImplementedError
