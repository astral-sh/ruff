# The decorator reads an alias whose specialization depends on the decorated function.
condition: bool


@(decorator := lambda _: Result)
def create():
    pass


Argument = create

if condition:
    type Result = decorator[Argument]
else:
    @create
    class Result[T]:
        pass


Result = 0 in [decorator] is (decorator := create)
