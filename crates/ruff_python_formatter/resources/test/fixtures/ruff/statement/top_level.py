class A:
    def __init__(self):
        pass

class B:
    def __init__(self):
        pass

def foo():
    pass

class Del(expr_context): ...
class Load(expr_context): ...

# Some comment.
class Other(expr_context): ...
class Store(expr_context): ...
class Foo(Bar): ...

class Baz(Qux):
    def __init__(self):
        pass

class Quux(Qux):
    def __init__(self):
        pass

# Some comment.
class Quuz(Qux):
    def __init__(self):
        pass

def bar(): ...
def baz(): ...
def quux():
    """Some docstring."""

def quuz():
    """Some docstring."""
