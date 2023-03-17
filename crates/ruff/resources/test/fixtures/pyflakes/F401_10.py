"""Test: imports within `ModuleNotFoundError` handlers."""


def check_orjson():
    try:
        import orjson

        return True
    except ModuleNotFoundError:
        return False
