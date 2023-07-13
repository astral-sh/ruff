from abc import ABCMeta


class Class:
    def __init_subclass__(self, default_name, **kwargs):
        ...

    @classmethod
    def badAllowed(self, x, /, other):
        ...

    @classmethod
    def stillBad(self, x, /, other):
        ...


class MetaClass(ABCMeta):
    def badAllowed(self):
        pass

    def stillBad(self):
        pass
