class StudentA:
    __slots__ = ("name",)

    def __init__(self, name, surname):
        self.name = name
        self.surname = surname  # [assigning-non-slot]
        self.setup()

    def setup(self):
        pass


class StudentB:
    __slots__ = ("name", "surname")

    def __init__(self, name, middle_name):
        self.name = name
        self.middle_name = middle_name  # [assigning-non-slot]
        self.setup()

    def setup(self):
        pass


class StudentC:
    __slots__ = ("name", "surname")

    def __init__(self, name, surname):
        self.name = name
        self.surname = surname  # OK
        self.setup()

    def setup(self):
        pass


class StudentD(object):
    __slots__ = ("name", "surname")

    def __init__(self, name, middle_name):
        self.name = name
        self.middle_name = middle_name  # [assigning-non-slot]
        self.setup()

    def setup(self):
        pass


class StudentE(StudentD):
    def __init__(self, name, middle_name):
        self.name = name
        self.middle_name = middle_name  # OK
        self.setup()

    def setup(self):
        pass


class StudentF(object):
    __slots__ = ("name", "__dict__")

    def __init__(self, name, middle_name):
        self.name = name
        self.middle_name = middle_name  # [assigning-non-slot]
        self.setup()

    def setup(self):
        pass


# https://github.com/astral-sh/ruff/issues/11358
class Foo:
    __slots__ = ("bar",)

    def __init__(self):
        self.qux = 2

    @property
    def qux(self):
        return self.bar * 2

    @qux.setter
    def qux(self, value):
        self.bar = value / 2


class StudentG:
    names = ("surname",)
    __slots__ = (*names, "a")

    def __init__(self, name, surname):
        self.name = name
        self.surname = surname  # [assigning-non-slot]
        self.setup()

    def setup(self):
        pass
