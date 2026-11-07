import sys  # ok: already at the top level


def needs_pandas():
    import pandas  # RUF078

    return pandas.DataFrame()


def needs_yaml():
    from yaml import safe_load  # RUF078

    return safe_load


def already_lazy():
    lazy import numpy  # ok: already lazy

    return numpy.arange(3)


class Loader:
    def load(self):
        from io import BytesIO  # RUF078

        return BytesIO()
