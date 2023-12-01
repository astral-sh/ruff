def func():
    return 1


def func():
    return 1.5


def func(x: int):
    if x > 0:
        return 1
    else:
        return 1.5


def func():
    return True


def func(x: int):
    if x > 0:
        return None
    else:
        return


def func(x: int):
    return 1 or 2.5 if x > 0 else 1.5 or "str"


def func(x: int):
    return 1 + 2.5 if x > 0 else 1.5 or "str"


def func(x: int):
    if not x:
        return None
    return {"foo": 1}


def func(x: int):
    return {"foo": 1}


def func(x: int):
    if not x:
        return 1
    else:
        return True


def func(x: int):
    if not x:
        return 1
    else:
        return None


def func(x: int):
    if not x:
        return 1
    elif x > 5:
        return "str"
    else:
        return None


def func(x: int):
    if x:
        return 1


def func():
    x = 1


def func(x: int):
    if x > 0:
        return 1


def func(x: int):
    match x:
        case [1, 2, 3]:
            return 1
        case 4 as y:
            return "foo"


def func(x: int):
    for i in range(5):
        if i > 0:
            return 1


def func(x: int):
    for i in range(5):
        if i > 0:
            return 1
    else:
        return 4


def func(x: int):
    for i in range(5):
        if i > 0:
            break
    else:
        return 4


def func(x: int):
    try:
        pass
    except:
        return 1


def func(x: int):
    try:
        pass
    except:
        return 1
    finally:
        return 2


def func(x: int):
    try:
        pass
    except:
        return 1
    else:
        return 2


def func(x: int):
    try:
        return 1
    except:
        return 2
    else:
        pass


def func(x: int):
    while x > 0:
        break
        return 1
