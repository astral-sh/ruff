class Wolf:
    @staticmethod
    def eat(self):  # [bad-staticmethod-argument]
        pass


class Wolf:
    @staticmethod
    def eat(sheep):
        pass


class Sheep:
    @staticmethod
    def eat(cls, x, y, z):  # [bad-staticmethod-argument]
        pass

    @staticmethod
    def sleep(self, x, y, z):  # [bad-staticmethod-argument]
        pass

    def grow(self, x, y, z):
        pass

    @classmethod
    def graze(cls, x, y, z):
        pass


class Foo:
    @staticmethod
    def eat(x, self, z):
        pass

    @staticmethod
    def sleep(x, cls, z):
        pass

    def grow(self, x, y, z):
        pass

    @classmethod
    def graze(cls, x, y, z):
        pass


class Foo:
    @staticmethod
    def __new__(cls, x, y, z):  # OK, see https://docs.python.org/3/reference/datamodel.html#basic-customization
        pass

# `__new__` is an implicit staticmethod, so this should still trigger (with
# `self` but not with `cls` as first argument - see above).
class Foo:
    def __new__(self, x, y, z):  # [bad-staticmethod-argument]
        pass
