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


def f(provided: int) -> int:
    match provided:
        case True:
            return captured  # F821
        case [captured, *_]:
            return captured
        case captured:
            return captured
