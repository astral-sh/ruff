"""Test: inner class annotation."""

class RandomClass:
    def bad_func(self) -> InnerClass: ...  # F821
    def good_func(self) -> OuterClass.InnerClass: ...  # Okay

class OuterClass:
    class InnerClass: ...

    def good_func(self) -> InnerClass: ...  # Okay
