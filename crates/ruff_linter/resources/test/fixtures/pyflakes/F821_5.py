"""Test: inner class annotation."""


class RandomClass:
    def random_func(self) -> "InnerClass":
        pass


class OuterClass:
    class InnerClass:
        pass

    def failing_func(self) -> "InnerClass":
        return self.InnerClass()
