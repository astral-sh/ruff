a = 1
b: int  # Considered a binding in a `.pyi` stub file, not in a `.py` runtime file

__all__ = ["a", "b", "c"]  # c is flagged as missing; b is not
