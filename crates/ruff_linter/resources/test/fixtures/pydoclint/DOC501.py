# https://github.com/astral-sh/ruff/issues/12428
def parse_bool(x, default=_parse_bool_sentinel):
    """Parse a boolean value
    bool or type(default)
    Raises
    `ValueError`
   ê>>> all(parse_bool(x) for x in [True, "yes", "Yes", "true", "True", "on", "ON", "1", 1])
    """
