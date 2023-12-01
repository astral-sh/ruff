def a():
    yield

def __init__():
    yield

class A:
    def __init__(self):
        yield


class B:
    def __init__(self):
        yield from self.gen()

    def gen(self):
        yield 5
