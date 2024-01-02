class Animal:
    @staticmethod
    def speak():
        return f"This animal says something."


class BadDog(Animal):
    @staticmethod
    def speak():
        original_speak = super.speak()  # PLW0245
        return f"{original_speak} But as a dog, it barks!"


class GoodDog(Animal):
    @staticmethod
    def speak():
        original_speak = super().speak()  # OK
        return f"{original_speak} But as a dog, it barks!"


class FineDog(Animal):
    @staticmethod
    def speak():
        super = "super"
        original_speak = super.speak()  # OK
        return f"{original_speak} But as a dog, it barks!"


def super_without_class() -> None:
    super.blah()  # OK


super.blah()  # OK
