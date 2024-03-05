from random import choice

class Fruit:
    COLORS = []

    def __init__(self, color):
        self.color = color

    def pick_colors(cls, *args):  # [no-classmethod-decorator]
        """classmethod to pick fruit colors"""
        cls.COLORS = args

    pick_colors = classmethod(pick_colors)

    def pick_one_color():  # [no-staticmethod-decorator]
        """staticmethod to pick one fruit color"""
        return choice(Fruit.COLORS)

    pick_one_color = staticmethod(pick_one_color)

class Class:
    def class_method(cls):
        pass

    class_method = classmethod(class_method);another_statement

    def static_method():
        pass

    static_method = staticmethod(static_method);
