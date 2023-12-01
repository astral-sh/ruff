"""Test: imports within `ModuleNotFoundError` and `ImportError` handlers."""


def module_not_found_error():
    try:
        import orjson

        return True
    except ModuleNotFoundError:
        return False


def import_error():
    try:
        import orjson

        return True
    except ImportError:
        return False
