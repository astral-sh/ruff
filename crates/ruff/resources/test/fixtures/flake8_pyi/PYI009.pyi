def bar(): ...  # OK
def foo():
    pass  # ERROR PYI009, since we're in a stub file

class Bar: ...  # OK

class Foo:
    pass  # ERROR PYI009, since we're in a stub file
