import random


class Class:
    def bad_method(this):
        pass

    if random.random(0, 2) == 0:

        def extra_bad_method(this):
            pass

    def good_method(self):
        pass

    @classmethod
    def class_method(cls):
        pass

    @staticmethod
    def static_method(x):
        return x


def func(x):
    return x
