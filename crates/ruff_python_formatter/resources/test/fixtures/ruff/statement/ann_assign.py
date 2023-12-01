# Regression test: Don't forget the parentheses in the value when breaking
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa: int = a + 1 * a

bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb: Bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb = (
    Bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb()
)

bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb: (
    Bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
)= Bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb()

JSONSerializable: TypeAlias = (
    "str | int | float | bool | None | list | tuple | JSONMapping"
)

JSONSerializable: str | int | float | bool | None | list | tuple | JSONMapping = {1, 2, 3, 4}

JSONSerializable: str | int | float | bool | None | list | tuple | JSONMapping = aaaaaaaaaaaaaaaa

# Regression test: Don't forget the parentheses in the annotation when breaking
class DefaultRunner:
    task_runner_cls: TaskRunnerProtocol | typing.Callable[[], typing.Any] = DefaultTaskRunner
