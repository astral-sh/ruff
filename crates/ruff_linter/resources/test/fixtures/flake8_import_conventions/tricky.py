"""Test cases for difficult renames."""


def rename_global():
    try:
        global pandas
        import pandas
    except ImportError:
        return False
