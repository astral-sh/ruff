import collections
from collections import namedtuple
from typing import Type, TypeAlias, TypeVar, NewType, NamedTuple, TypedDict

GLOBAL: str = "foo"


def assign():
    global GLOBAL
    GLOBAL = "bar"
    lower = 0
    Camel = 0
    CONSTANT = 0
    _ = 0

    MyObj1 = collections.namedtuple("MyObj1", ["a", "b"])
    MyObj2 = namedtuple("MyObj12", ["a", "b"])

    T = TypeVar("T")
    UserId = NewType("UserId", int)

    Employee = NamedTuple("Employee", [("name", str), ("id", int)])

    Point2D = TypedDict("Point2D", {"in": int, "x-y": int})

    IntOrStr: TypeAlias = int | str

    type MyInt = int


def aug_assign(rank, world_size):
    global CURRENT_PORT

    CURRENT_PORT += 1
    if CURRENT_PORT > MAX_PORT:
        CURRENT_PORT = START_PORT


def loop_assign():
    global CURRENT_PORT
    for CURRENT_PORT in range(5):
        pass


def model_assign() -> None:
    Bad = apps.get_model("zerver", "Stream")  # N806
    Attachment = apps.get_model("zerver", "Attachment")  # OK
    Recipient = apps.get_model("zerver", model_name="Recipient")  # OK
    Address: Type = apps.get_model("zerver", "Address")  # OK

    from django.utils.module_loading import import_string

    Bad = import_string("django.core.exceptions.ValidationError")  # N806
    ValidationError = import_string("django.core.exceptions.ValidationError")  # OK

    Bad = apps.get_model()  # N806
    Bad = apps.get_model(model_name="Stream")  # N806

    Address: Type = apps.get_model("zerver", variable)  # OK
    ValidationError = import_string(variable)  # N806
