# this is invalid in versions below py312
class ClassA[T: str]:
    def method1(self) -> T:
        ...
