from functools import singledispatch, singledispatchmethod


@singledispatch
def convert_position(position):
    pass

class Board:
    @singledispatch  # [singledispatch-method]
    @classmethod
    def convert_position(cls, position):
        pass

    @singledispatch  # [singledispatch-method]
    def move(self, position):
        pass

    @singledispatchmethod
    def place(self, position):
        pass
