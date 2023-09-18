# Regression test for branch detection from
# https://github.com/pypa/build/blob/5800521541e5e749d4429617420d1ef8cdb40b46/src/build/_importlib.py
import sys

if sys.version_info < (3, 8):
    import importlib_metadata as metadata
elif sys.version_info < (3, 9, 10) or (3, 10, 0) <= sys.version_info < (3, 10, 2):
    try:
        import importlib_metadata as metadata
    except ModuleNotFoundError:
        from importlib import metadata
else:
    from importlib import metadata

__all__ = ["metadata"]
