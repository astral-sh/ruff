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

D: (E := 4) = (F := 5)

# Regression test: Don't forget the parentheses in the annotation when breaking
class DefaultRunner:
    task_runner_cls: TaskRunnerProtocol | typing.Callable[[], typing.Any] = DefaultTaskRunner


# Regression test: Preserve parentheses around invalid type expressions.
def preserve_invalid_type_expressions_in_annotations():
    named: (value := int) = 1
    yielded_with_value: (yield 1) = 1
    yielded: (yield 1)


async def preserve_invalid_type_expressions_in_async_annotations():
    awaited: (await g()) = 1
    bare_awaited: (await g())
