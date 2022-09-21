def get_name():
    return self.name


def get_name():
    return (self.name,)


def get_name():
    del self.name


def get_name(self):
    return self.name


x = list()


def randdec(maxprec, maxexp):
    return numeric_string(maxprec, maxexp)


def ternary_optarg(prec, exp_range, itr):
    for _ in range(100):
        a = randdec(prec, 2 * exp_range)
        b = randdec(prec, 2 * exp_range)
        c = randdec(prec, 2 * exp_range)
        yield a, b, c, None
        yield a, b, c, None, None


class Foo:
    CLASS_VAR = 1
    REFERENCES_CLASS_VAR = {"CLASS_VAR": CLASS_VAR}
    ANNOTATED_CLASS_VAR: int = 2


from typing import Literal


class Class:
    copy_on_model_validation: Literal["none", "deep", "shallow"]
    post_init_call: Literal["before_validation", "after_validation"]

    def __init__(self):
        Class


try:
    x = 1 / 0
except Exception as e:
    print(e)


y: int = 1

x: "Bar" = 1

[first] = ["yup"]


from typing import List, TypedDict


class Item(TypedDict):
    nodes: List[TypedDict("Node", {"name": str})]


from enum import Enum


class Ticket:
    class Status(Enum):
        OPEN = "OPEN"
        CLOSED = "CLOSED"

    def set_status(self, status: Status):
        self.status = status


def update_tomato():
    print(TOMATO)
    TOMATO = "cherry tomato"


A = f'{B}'
A = (
    f'B'
    f'{B}'
)
