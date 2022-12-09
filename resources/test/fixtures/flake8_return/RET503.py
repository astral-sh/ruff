###
# Errors
###

# if/elif/else
def x(y):
    if not y:
        return 1
    # error


def x(y):
    if not y:
        print()  # error
    else:
        return 2


def x(y):
    if not y:
        return 1

    print()  # error


# for
def x(y):
    for i in range(10):
        if i > 10:
            return i
    # error


def x(y):
    for i in range(10):
        if i > 10:
            return i
    else:
        print()  # error


###
# Non-errors
###

# raise as last return
def x(y):
    if not y:
        return 1
    raise Exception


# last line in while loop
def x(y):
    while True:
        if y > 0:
            return 1
        y += 1


# exclude empty functions
def x(y):
    return None


# return inner with statement
def x(y):
    with y:
        return 1


async def function():
    async with thing as foo:
        return foo.bar


# assert as last return
def x(y):
    if not y:
        return 1
    assert False, "Sanity check"


# return value within loop
def bar1(x, y, z):
    for i in x:
        if i > y:
            break
        return z


def bar3(x, y, z):
    for i in x:
        if i > y:
            if z:
                break
        else:
            return z
        return None


def bar1(x, y, z):
    for i in x:
        if i < y:
            continue
        return z


def bar3(x, y, z):
    for i in x:
        if i < y:
            if z:
                continue
        else:
            return z
        return None
