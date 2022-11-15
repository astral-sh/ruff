def trivial():
    pass


def expr_as_statement():
    0xF00D


def sequential(n):
    k = n + 4
    s = k + n
    return s


def if_elif_else_dead_path(n):
    if n > 3:
        return "bigger than three"
    elif n > 4:
        return "is never executed"
    else:
        return "smaller than or equal to three"


def nested_ifs():
    if n > 3:
        if n > 4:
            return "bigger than four"
        else:
            return "bigger than three"
    else:
        return "smaller than or equal to three"


def for_loop():
    for i in range(10):
        print(i)


def for_else(mylist):
    for i in mylist:
        print(i)
    else:
        print(None)


def recursive(n):
    if n > 4:
        return f(n - 1)
    else:
        return n


def nested_functions():
    def a():
        def b():
            pass
        b()
    a()


def try_else():
    try:
        print(1)
    except TypeA:
        print(2)
    except TypeB:
        print(3)
    else:
        print(4)


def nested_try_finally():
    try:
        try:
            print(1)
        finally:
            print(2)
    finally:
        print(3)


async def foobar(a, b, c):
    await whatever(a, b, c)
    if await b:
        pass
    async with c:
        pass
    async for x in a:
        pass


def annotated_assign():
    x: Any = None
