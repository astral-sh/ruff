class Soup:
    @staticmethod
    def temp() -> None:
        print("Soup is hot!")


class TomatoSoup(Soup):
    @staticmethod
    def temp() -> None:
        super.temp()  # PLW0245
        print("But tomato soup is even hotter!")

class ProperTomatoSoup(Soup):
    @staticmethod
    def temp() -> None:
        super().temp()  # OK
        print("But tomato soup is even hotter!")


class SuperSoup(Soup):
    @staticmethod
    def temp() -> None:
        # override the super builtin
        super = "super"
        super.temp()  # OK (according to this rule, at least)
        print(f"But super soup is {super}!")
