class Class:
    @classmethod
    def bad_class_method(this):
        pass

    @classmethod
    def good_class_method(cls):
        pass

    def method(self):
        pass

    @staticmethod
    def static_method(x):
        return x


def func(x):
    return x
