def f1(value=["""first
second"""]):
    return value


def f2(value={"key": """first
second
third"""}):
    return value


def f3(value=["""first
second"""]):
    """Docstring, insertion happens after this."""
    return value
