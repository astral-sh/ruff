# regression test for https://github.com/astral-sh/ty/issues/2085

class Foo:
    def __init__(self, x: int):
        self.left = x
        self.right = x
    def method(self):
        self.left, self.right = self.right, self.left
        if self.right:
            self.right = self.right
