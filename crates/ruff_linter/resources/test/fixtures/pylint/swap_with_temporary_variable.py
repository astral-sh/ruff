# safe fix
def foo(x: int, y: int):
    temp: int = x
    x = y
    y = temp


# safe fix inside an if condition
def foo(x: int, y: int):
    if x > 5:
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


# not a simple swap statement because temp variable is re-used later
def foobar(x: int, y: int):
    temp = x
    x = y
    y = temp

    # use temp variable again,
    # so its declaration can't be removed
    z = temp


# not a simple swap statement because the temp variable is global
swap_var = 0


def quux(x: int, y: int):
    global swap_var
    swap_var = x
    x = y
    y = swap_var


# temp is read somewhere else in the code, so this is not a simple swap statement and hence ignored
def foo(x, y):
    temp = []

    def bar():
        print(temp)

    temp = x
    x = y
    y = temp
    bar()
