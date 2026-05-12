class Pair:
    def __init__(self, left: int, right: int) -> None:
        self.right = right
        self.left = left
        self.alias = left


pair = Pair(4, 9)
print(pair.left)
print(pair.right)
print(pair.alias)
