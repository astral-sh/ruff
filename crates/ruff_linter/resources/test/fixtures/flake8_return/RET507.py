###
# Errors
###
def foo2(x, y, w, z):
    for i in x:
        if i < y:  # [no-else-continue]
            continue
        elif i < w:
            continue
        else:
            a = z


def foo5(x, y, z):
    for i in x:
        if i < y:  # [no-else-continue]
            if y:
                a = 4
            else:
                b = 2
            continue
        elif z:
            c = 2
        else:
            c = 3
        continue


###
# Non-errors
###
def foo1(x, y, z):
    for i in x:
        if i < y:  # [no-else-continue]
            continue
        else:
            a = z


def foo3(x, y, z):
    for i in x:
        if i < y:
            a = 1
            if z:  # [no-else-continue]
                b = 2
                continue
            else:
                c = 3
                continue
        else:
            d = 4
            continue


def foo4(x, y):
    for i in x:
        if i < x:  # [no-else-continue]
            if y:
                a = 4
            else:
                b = 2
            continue
        else:
            c = 3
        continue


def foo6(x, y):
    for i in x:
        if i < x:
            if y:  # [no-else-continue]
                a = 4
                continue
            else:
                b = 2
        else:
            c = 3
        continue


def bar4(x):
    for i in range(10):
        if x:  # [no-else-continue]
            continue
        else:
            try:
                return
            except ValueError:
                continue


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
