"""foo"""  # ERROR PYI021

def foo():
    """foo"""  # ERROR PYI021

class Bar:
    """bar"""  # ERROR PYI021

def bar():
    x = 1
    """foo"""  # OK, not a doc string
