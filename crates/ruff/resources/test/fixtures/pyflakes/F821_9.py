"""Test: match statements."""
from dataclasses import dataclass


@dataclass
class Car:
    make: str
    model: str


def f():
    match Car("Toyota", "Corolla"):
        case Car("Toyota", model):
            print(model)
        case Car(make, "Corolla"):
            print(make)
        case Car(_, _):
            print(make, model)
        case _:
            print("No match")


def f(provided: int) -> int:
    match provided:
        case True:
            return captured  # F821
        case [*_] as captured:
            return captured
        case [captured, *_]:
            return captured
        case captured:
            return captured


match 1:
    case 1:
        x = 1

print(x)
