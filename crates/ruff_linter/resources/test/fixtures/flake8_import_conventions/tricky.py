"""Test cases for difficult renames."""

import datetime


def rename_global():
    try:
        global pandas
        import pandas
    except ImportError:
        return False


def shadowed_in_inner_scope(seconds: float) -> None:
    dt = datetime.datetime.fromtimestamp(seconds, tz=datetime.timezone.utc)
    print(dt)
