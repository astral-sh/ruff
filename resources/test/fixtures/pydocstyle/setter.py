class PropertyWithSetter:
    @property
    def foo(self) -> str:
        """Docstring for foo."""
        return "foo"

    @foo.setter
    def foo(self, value: str) -> None:
        pass

    @foo.deleter
    def foo(self):
        pass

    @foo
    def foo(self, value: str) -> None:
        pass
