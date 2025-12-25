# safe fix
def foo(x: int, y: int):
    temp: int = x
    x = y
    y = temp


# unsafe fix because the swap statements contain a comment
def bar(x: int, y: int):
    temp: int = x  # comment
    x = y
    y = temp


# not a swap statement
def baz(x: int, y: int):
    temp = x
    x = y
    y = x
