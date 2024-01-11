class MyClass:
    ImportError = 4
    id: int
    dir = "/"

    def __init__(self):
        self.float = 5  # is fine
        self.id = 10
        self.dir = "."

    def int(self):
        pass

    def str(self):
        pass

    def method_usage(self) -> str:
        pass

    def attribute_usage(self) -> id:
        pass
