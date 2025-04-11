def f():
    return 1  # okay


def f():
    return  # okay


async def f():
    return  # okay


return 1  # error
return  # error


class C:
    return 1  # error


def f():
    class C:
        return 1  # error
