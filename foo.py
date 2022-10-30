def function_with_nesting():
    """Foo bar documentation."""
    @overload
    def nested_overloaded_func(a: int) -> str:
        ...
