class Fruit:
    @classmethod
    def list_fruits(cls) -> None:
        cls = "apple"
        cls: Fruit = "apple"
        cls += "orange"
        cls, blah = "apple", "orange"

    def print_color(self) -> None:
        self = "red"
        self: Self = "red"
        self += "blue"
        self, blah = "red", "blue"
    
    def ok(self) -> None:
        cls = None  # OK because the rule looks for the name in the signature
