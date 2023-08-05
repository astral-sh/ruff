x = 1  # type: ignore
x = 1  # type:ignore
x = 1  # type: ignore[attr-defined]  # type: ignore
x = 1  # type: ignoreme # type: ignore

x = 1
x = 1  # type ignore
x = 1  # type ignore  # noqa
x = 1  # type: ignore[attr-defined]
x = 1  # type: ignore[attr-defined, name-defined]
x = 1  # type: ignore[attr-defined]  # type: ignore[type-mismatch]
x = 1  # type: ignore[type-mismatch]  # noqa
x = 1  # type: ignore [attr-defined]
x = 1  # type: ignore [attr-defined, name-defined]
x = 1  # type: ignore [type-mismatch]  # noqa
x = 1  # type: Union[int, str]
x = 1  # type: ignoreme
