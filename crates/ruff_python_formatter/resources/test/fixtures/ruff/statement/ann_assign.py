# Regression test: Don't forget the parentheses in the value when breaking
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa: int = a + 1 * a


# Regression test: Don't forget the parentheses in the annotation when breaking
class DefaultRunner:
    task_runner_cls: TaskRunnerProtocol | typing.Callable[[], typing.Any] = DefaultTaskRunner
