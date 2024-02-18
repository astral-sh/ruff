class A:  # CNL100
    def __init__(self):
        pass


class B:  # correct

    def __init__(self):
        pass


class C:  # CNL100
    async def foo(self):
        pass


class D:  # correct

    async def foo(self):
        pass
