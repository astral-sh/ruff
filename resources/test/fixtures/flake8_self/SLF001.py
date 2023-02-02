class Foo:

    def __init__(self):
        self.public_thing = "foo"
        self._private_thing = "bar"
        self.__really_private_thing = "baz"

    def __str__(self):
        return "foo"

    def public_func(self):
        pass

    def _private_func(self):
        pass

    def __really_private_func(self, arg):
        pass


foo = Foo()

print(foo.public_thing)
print(foo.public_func())
print(foo.__dict__)
print(foo.__str__())

print(foo._private_thing)  # SLF001
print(foo.__really_private_thing)  # SLF001
print(foo._private_func())  # SLF001
print(foo.__really_private_func(1))  # SLF001
