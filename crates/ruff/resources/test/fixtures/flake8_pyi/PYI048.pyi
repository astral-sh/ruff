def bar():
    ...  # OK


def oof():   # OK, docstrings are handled by another rule
  """oof"""
  print("foo")



def foo():   # ERROR PYI048
    """foo"""
    print("foo")
    print("foo")


def buzz():   # ERROR PYI048
    print("fizz")
    print("buzz")
    print("test")
