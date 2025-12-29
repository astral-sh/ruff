obj = ClassA[str]()
class ClassA[T: (str, bytes)](BaseClass[T]):
    attr1: T
class BaseClass[T]:
    pass
