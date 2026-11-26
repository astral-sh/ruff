"""Test cases for difficult renames."""


def rename_global():
    try:
        global pandas
        import pandas
    except ImportError:
        return False


# The expected alias is a local variable in the scope of a reference.
import datetime


def format_time(record):
    dt = datetime.datetime.fromtimestamp(record.created, tz=datetime.UTC)
    return dt.isoformat()


def parameter_collision():
    import pandas

    def frame(pd):
        return pandas.DataFrame(pd)

    return frame


def enclosing_scope_collision():
    import numpy

    def outer():
        np = 1

        def inner():
            return numpy.array([np])

        return inner

    return outer


def alias_available():
    import pandas

    def frame(data):
        return pandas.DataFrame(data)

    return frame
