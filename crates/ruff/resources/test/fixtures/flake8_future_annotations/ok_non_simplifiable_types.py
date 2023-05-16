from typing import NamedTuple


class Stuff(NamedTuple):
    x: int


def main() -> None:
    a_list = Stuff(5)
    print(a_list)
