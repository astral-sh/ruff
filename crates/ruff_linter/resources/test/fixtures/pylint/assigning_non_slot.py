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

    def __init__(self, name, surname):
        self.name = name
        self.surname = surname # OK
        self.setup()

    def setup(self):
        pass
