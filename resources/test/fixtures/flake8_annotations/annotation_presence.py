# Error
def foo(a, b):
    pass


# Error
def foo(a: int, b):
    pass


# Error
def foo(a: int, b) -> int:
    pass


# Error
def foo(a: int, b: int):
    pass


# Error
def foo():
    pass


# OK
def foo(a: int, b: int) -> int:
    pass


# OK
def foo() -> int:
    pass
