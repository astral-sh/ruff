# Complexity = 1
def trivial():
    pass


# Complexity = 1
def expr_as_statement():
    0xF00D


# Complexity = 1
def sequential(n):
    k = n + 4
    s = k + n
    return s


# Complexity = 3
def if_elif_else_dead_path(n):
    if n > 3:
        return "bigger than three"
    elif n > 4:
        return "is never executed"
    else:
        return "smaller than or equal to three"


# Complexity = 3
def nested_ifs():
    if n > 3:
        if n > 4:
            return "bigger than four"
        else:
            return "bigger than three"
    else:
        return "smaller than or equal to three"


# Complexity = 2
def for_loop():
    for i in range(10):
        print(i)


# Complexity = 2
def for_else(mylist):
    for i in mylist:
        print(i)
    else:
        print(None)


# Complexity = 2
def recursive(n):
    if n > 4:
        return f(n - 1)
    else:
        return n


# Complexity = 3
def nested_functions():
    def a():
        def b():
            pass

        b()

    a()


# Complexity = 4
def try_else():
    try:
        print(1)
    except TypeA:
        print(2)
    except TypeB:
        print(3)
    else:
        print(4)


# Complexity = 3
def nested_try_finally():
    try:
        try:
            print(1)
        finally:
            print(2)
    finally:
        print(3)


# Complexity = 3
async def foobar(a, b, c):
    await whatever(a, b, c)
    if await b:
        pass
    async with c:
        pass
    async for x in a:
        pass


# Complexity = 1
def annotated_assign():
    x: Any = None


# Complexity = 9
class Class:
    def handle(self, *args, **options):
        if args:
            return

        class ServiceProvider:
            def a(self):
                pass

            def b(self, data):
                if not args:
                    pass

        class Logger:
            def c(*args, **kwargs):
                pass

            def error(self, message):
                pass

            def info(self, message):
                pass

            def exception(self):
                pass

        return ServiceProvider(Logger())
