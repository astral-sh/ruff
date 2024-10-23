# Check for E30 errors in a file containing syntax errors with unclosed
# parenthesis.

def foo[T1, T2():
    pass

def bar():
    pass



class Foo:
    def __init__(
        pass
    def method():
        pass

foo = Foo(


def top(
    def nested1():
        pass
    def nested2():
        pass

