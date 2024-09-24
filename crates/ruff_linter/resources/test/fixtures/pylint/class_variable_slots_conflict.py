class Person:
    __slots__ = ("age", "name", "say_hi")
    name = None

    def __init__(self, age, name):
        self.age = age
        self.name = name

    @property
    def age(self):
        return self.age

    def say_hi(self):
        print(f"Hi, I'm {self.name}.")
