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


# no fix because temp variable is re-used later
def foobar(x: int, y: int):
    temp = x
    x = y
    y = temp

    # use temp variable again,
    # so its declaration can't be removed
    z = temp


# no fix because the temp variable is global
# swap_var = 0


def quux(x: int, y: int):
    swap_var = x
    x = y
    y = swap_var
