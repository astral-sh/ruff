def f(a, b):
    print("Hello, world!")


def f(a, b, *args, **kwargs):
    print("Hello, world!")


class C:
    def f(self, a, b):
        print("Hello, world!")

    def f(self, a, b, *args, **kwargs):
        print("Hello, world!")
