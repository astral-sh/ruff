from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from threading import Thread
    from pandas import DataFrame, Series
    import pandas as pd


def fires_on_param(t: Thread):
    return t


def fires_on_return() -> Thread:
    ...


def fires_in_subscript(x: list[Thread]):
    return x


def fires_in_union(x: Thread | int):
    return x


def fires_on_attribute(x: pd.DataFrame):
    return x


module_level: Thread


class Klass:
    field: Thread

    def method(self, t: Thread):
        return t


def multiple_references(x: Thread, y: Series, z: DataFrame):
    return x, y, z


def local_annotation_ok():
    t: Thread = None
    return t


def already_quoted_ok(t: "Thread"):
    return t


def already_quoted_subscript_ok(x: "list[Thread]"):
    return x


def runtime_import_ok(x: int):
    return x


def already_triple_quoted_ok(x: """Thread"""):
    return x


def parenthesized_reference(x: (Thread)):
    return x


def parenthesized_multiline(
    x: (
        Thread
    ),
):
    return x
