from __future__ import annotations

def foo():
    A = 1
    class C:
        @classmethod
        def f(cls, x: A) -> C:
            y: A = 1
            return cls()
