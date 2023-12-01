class MyClass:
    ImportError = 4
    id: int
    dir = "/"

    def __init__(self):
        self.float = 5  # is fine
        self.id = 10
        self.dir = "."

    def str(self):
        pass


from typing import TypedDict


class MyClass(TypedDict):
    id: int


from threading import Event


class CustomEvent(Event):
    def set(self) -> None:
        ...

    def str(self) -> None:
        ...


from logging import Filter, LogRecord


class CustomFilter(Filter):
    def filter(self, record: LogRecord) -> bool:
        ...

    def str(self) -> None:
        ...


from typing_extensions import override


class MyClass:
    @override
    def str(self):
        pass

    def int(self):
        pass
