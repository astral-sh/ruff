class Person:
    def __init__(self):
        self.name = "monty"

    def __eq__(self, other):
        return isinstance(other, Person) and other.name == self.name

class Language:
    def __init__(self):
        self.name = "python"

    def __eq__(self, other):
        return isinstance(other, Language) and other.name == self.name

    def __hash__(self):
        return hash(self.name)
