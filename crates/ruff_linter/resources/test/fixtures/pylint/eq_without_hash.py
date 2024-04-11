class Person:  # [eq-without-hash]
    def __init__(self):
        self.name = "monty"

    def __eq__(self, other):
        return isinstance(other, Person) and other.name == self.name


# OK
class Language:
    def __init__(self):
        self.name = "python"

    def __eq__(self, other):
        return isinstance(other, Language) and other.name == self.name

    def __hash__(self):
        return hash(self.name)


class MyClass:
    def __eq__(self, other):
        return True

    __hash__ = None


class SingleClass:
    def __eq__(self, other):
        return True

    def __hash__(self):
        return 7


class ChildClass(SingleClass):
    def __eq__(self, other):
        return True

    __hash__ = SingleClass.__hash__
