def bar():
    ...  # OK


def oof():
  """oof"""
  print("foo")  # OK, docstrings are handled by another rule



def foo():
    """foo"""
    print("foo")
    print("foo")  # ERROR PYI048


def buzz():
    print("fizz")
    print("buzz")  # ERROR PYI048
    print("test")  # ERROR PYI048
