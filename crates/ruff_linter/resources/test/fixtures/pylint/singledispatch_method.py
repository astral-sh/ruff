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

    @singledispatch  # [singledispatch-method]
    @staticmethod
    def do(position):
        pass

    # False negative (flagged by Pylint).
    @convert_position.register
    @classmethod
    def _(cls, position: str) -> tuple:
        position_a, position_b = position.split(",")
        return (int(position_a), int(position_b))

    # False negative (flagged by Pylint).
    @convert_position.register
    @classmethod
    def _(cls, position: tuple) -> str:
        return f"{position[0]},{position[1]}"
