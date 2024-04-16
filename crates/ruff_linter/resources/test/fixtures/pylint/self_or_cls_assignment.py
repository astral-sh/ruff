class Fruit:
    @classmethod
    def list_fruits(cls) -> None:
        cls = "apple"  # PLW0642
        cls: Fruit = "apple"  # PLW0642
        cls += "orange"  # PLW0642
        *cls = "banana"  # PLW0642
        cls, blah = "apple", "orange"  # PLW0642
        blah, (cls, blah2) = "apple", ("orange", "banana")  # PLW0642
        blah, [cls, blah2] = "apple", ("orange", "banana")  # PLW0642

    @classmethod
    def add_fruits(cls, fruits, /) -> None:
        cls = fruits  # PLW0642

    def print_color(self) -> None:
        self = "red"  # PLW0642
        self: Self = "red"  # PLW0642
        self += "blue"  # PLW0642
        *self = "blue"  # PLW0642
        self, blah = "red", "blue"  # PLW0642
        blah, (self, blah2) = "apple", ("orange", "banana")  # PLW0642
        blah, [self, blah2] = "apple", ("orange", "banana")  # PLW0642

    def print_color(self, color, /) -> None:
        self = color

    def ok(self) -> None:
        cls = None  # OK because the rule looks for the name in the signature

    @classmethod
    def ok(cls) -> None:
        self = None

    @staticmethod
    def list_fruits_static(self, cls) -> None:
        self = "apple"  # Ok
        cls = "banana"  # Ok


def list_fruits(self, cls) -> None:
    self = "apple"  # Ok
    cls = "banana"  # Ok
