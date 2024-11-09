"""foo"""  # ERROR PYI021

def foo():
    """foo"""  # ERROR PYI021

class Bar:
    """bar"""  # ERROR PYI021

class Qux:
    """qux"""  # ERROR PYI021

    def __init__(self) -> None: ...

class Baz:
    """Multiline docstring

    Lorem ipsum dolor sit amet
    """

    def __init__(self) -> None: ...

def bar():
    x = 1
    """foo"""  # OK, not a doc string
