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
