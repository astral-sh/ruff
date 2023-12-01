###
# Error
###
def foo2(x, y, w, z):
    if x:  # [no-else-return]
        a = 1
        return y
    elif z:
        b = 2
        return w
    else:
        c = 3
        return z


def foo5(x, y, z):
    if x:  # [no-else-return]
        if y:
            a = 4
        else:
            b = 2
        return
    elif z:
        c = 2
    else:
        c = 3
    return


def foo2(x, y, w, z):
    if x:
        a = 1
    elif z:
        b = 2
    else:
        c = 3

    if x:  # [no-else-return]
        a = 1
        return y
    elif z:
        b = 2
        return w
    else:
        c = 3
        return z


def foo1(x, y, z):
    if x:  # [no-else-return]
        a = 1
        return y
    else:
        b = 2
        return z


def foo3(x, y, z):
    if x:
        a = 1
        if y:  # [no-else-return]
            b = 2
            return y
        else:
            c = 3
            return x
    else:
        d = 4
        return z


def foo4(x, y):
    if x:  # [no-else-return]
        if y:
            a = 4
        else:
            b = 2
        return
    else:
        c = 3
    return


def foo6(x, y):
    if x:
        if y:  # [no-else-return]
            a = 4
            return
        else:
            b = 2
    else:
        c = 3
    return


def bar4(x):
    if x:  # [no-else-return]
        return True
    else:
        try:
            return False
        except ValueError:
            return None


###
# Non-error
###
def bar1(x, y, z):
    if x:
        return y
    return z


def bar2(w, x, y, z):
    if x:
        return y
    if z:
        a = 1
    else:
        return w
    return None


def bar3(x, y, z):
    if x:
        if z:
            return y
    else:
        return z
    return None


x = 0

if x == 1:
    y = "a"
elif x == 2:
    y = "b"
else:
    y = "c"
