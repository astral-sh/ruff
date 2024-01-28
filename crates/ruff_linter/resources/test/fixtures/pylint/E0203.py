class Unicorn:
    def __init__(self, fluffiness_level):
        if self.fluffiness_level > 9000:
            print("It's OVER-FLUFFYYYY ! *crush glasses*")
        self.fluffiness_level = fluffiness_level


class FluffyUnicorn:
    def __init__(self, fluffiness_level):
        self.fluffiness_level = fluffiness_level
        if self.fluffiness_level > 9000:
            print("It's OVER-FLUFFYYYY ! *crush glasses*")


class F:
    # Although it is used earlier than defined, it is not a problem
    def get_attribute(self):
        return self.attribute

    def __init__(self):
        self.attribute = 0

    # This should not be reported because rule only checks for __init__
    def get_uninitialized_attribute(self):
        return self.uninitialized_attribute


class Base:
    def __init__(self):
        self.attribute = 0

class Derived(Base):
    def __init__(self):
        # Should be fine since it's defined in the parent class
        print(self.attribute)
