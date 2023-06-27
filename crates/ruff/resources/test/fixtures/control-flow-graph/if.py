def func():
    if False:
        return 0
    return 1

def func():
    if True:
        return 1
    return 0

def func():
    if False:
        return 0
    else:
        return 1

def func():
    if True:
        return 1
    else:
        return 0

def func():
    if False:
        return 0
    else:
        return 1
    return "unreachable"

def func():
    if True:
        return 1
    else:
        return 0
    return "unreachable"

def func():
    if True:
        if True:
            return 1
        return 2
    else:
        return 3
    return "unreachable2"

def func():
    if False:
        return 0

def func():
    if True:
        return 1

def func():
    if True:
        return 1
    elif False:
        return 2
    else:
        return 0

def func():
    if False:
        return 1
    elif True:
        return 2
    else:
        return 0

def func():
    if True:
        if False:
            return 0
        elif True:
            return 1
        else:
            return 2
        return 3
    elif True:
        return 4
    else:
        return 5
    return 6

def func():
    if False:
        return "unreached"
    elif False:
        return "also unreached"
    return "reached"
