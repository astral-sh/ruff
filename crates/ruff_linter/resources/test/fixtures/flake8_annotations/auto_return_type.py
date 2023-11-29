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


def func():
    if not x:
        return 1
    else:
        return True
