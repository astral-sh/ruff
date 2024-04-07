from functools import singledispatchmethod


@singledispatchmethod  # [singledispatchmethod-function]
def convert_position(position):
    pass


class Board:

    @singledispatchmethod  # Ok
    @classmethod
    def convert_position(cls, position):
        pass

    @singledispatchmethod  # Ok
    def move(self, position):
        pass

    @singledispatchmethod  # Ok
    @staticmethod
    def do(position):
        pass
