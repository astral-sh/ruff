class MyClass:
    ImportError = 4
    id: int
    dir = "/"

    def __init__(self):
        self.float = 5  # is fine
        self.id = 10
        self.dir = "."

    def str(self):
        pass


from typing import TypedDict


class MyClass(TypedDict):
    id: int
