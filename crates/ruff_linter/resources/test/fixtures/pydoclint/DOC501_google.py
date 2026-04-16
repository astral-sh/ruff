import something
from somewhere import AnotherError


class FasterThanLightError(Exception):
    ...


_some_error = Exception


# OK
def calculate_speed(distance: float, time: float) -> float:
    """Calculate speed as distance divided by time.

    Args:
        distance: Distance traveled.
        time: Time spent traveling.

    Returns:
        Speed as distance divided by time.

    Raises:
        FasterThanLightError: If speed is greater than the speed of light.
    """
    try:
        return distance / time
    except ZeroDivisionError as exc:
        raise FasterThanLightError from exc


# DOC501
def calculate_speed(distance: float, time: float) -> float:
    """Calculate speed as distance divided by time.

    Args:
        distance: Distance traveled.
        time: Time spent traveling.

    Returns:
        Speed as distance divided by time.
    """
    try:
        return distance / time
    except ZeroDivisionError as exc:
        raise FasterThanLightError from exc


# DOC501
def calculate_speed(distance: float, time: float) -> float:
    """Calculate speed as distance divided by time.

    Args:
        distance: Distance traveled.
        time: Time spent traveling.

    Returns:
        Speed as distance divided by time.
    """
    try:
        return distance / time
    except ZeroDivisionError as exc:
        raise FasterThanLightError from exc
    except:
        raise ValueError


# DOC501
def calculate_speed(distance: float, time: float) -> float:
    """Calculate speed as distance divided by time.

    Args:
        distance: Distance traveled.
        time: Time spent traveling.

    Returns:
        Speed as distance divided by time.
    """
    try:
        return distance / time
    except ZeroDivisionError as exc:
        print('oops')
        raise exc


# DOC501
def calculate_speed(distance: float, time: float) -> float:
    """Calculate speed as distance divided by time.

    Args:
        distance: Distance traveled.
        time: Time spent traveling.

    Returns:
        Speed as distance divided by time.
    """
    try:
        return distance / time
    except (ZeroDivisionError, ValueError) as exc:
        print('oops')
        raise exc


# DOC501
def calculate_speed(distance: float, time: float) -> float:
    """Calculate speed as distance divided by time.

    Args:
        distance: Distance traveled.
        time: Time spent traveling.

    Returns:
        Speed as distance divided by time.
    """
    raise AnotherError


# DOC501
def calculate_speed(distance: float, time: float) -> float:
    """Calculate speed as distance divided by time.

    Args:
        distance: Distance traveled.
        time: Time spent traveling.

    Returns:
        Speed as distance divided by time.
    """
    raise AnotherError()


# DOC501
def foo(bar: int):
    """Foo.

    Args:
        bar: Bar.
    """
    raise something.SomeError


# DOC501, but can't resolve the error
def calculate_speed(distance: float, time: float) -> float:
    """Calculate speed as distance divided by time.

    Args:
        distance: Distance traveled.
        time: Time spent traveling.

    Returns:
        Speed as distance divided by time.
    """
    raise _some_error


# OK
def calculate_speed(distance: float, time: float) -> float:
    try:
        return distance / time
    except ZeroDivisionError as exc:
        raise FasterThanLightError from exc


# OK
def calculate_speed(distance: float, time: float) -> float:
    raise NotImplementedError


# OK
def foo(bar: int):
    """Foo.

    Args:
        bar: Bar.

    Raises:
        SomeError: Wow.
    """
    raise something.SomeError


# OK
def foo(bar: int):
    """Foo.

    Args:
        bar: Bar.

    Raises:
        something.SomeError: Wow.
    """
    raise something.SomeError


# DOC501
def calculate_speed(distance: float, time: float) -> float:
    """Calculate speed as distance divided by time.

    Args:
        distance: Distance traveled.
        time: Time spent traveling.

    Returns:
        Speed as distance divided by time.

    Raises:
        TypeError: if you didn't pass a number for both parameters
    """
    try:
        return distance / time
    except ZeroDivisionError:
        print("Oh no, why would you divide something by zero?")
        raise
    except TypeError:
        print("Not a number? Shame on you!")
        raise


# This is fine
def calculate_speed(distance: float, time: float) -> float:
    """Calculate speed as distance divided by time.

    Args:
        distance: Distance traveled.
        time: Time spent traveling.

    Returns:
        Speed as distance divided by time.
    """
    try:
        return distance / time
    except Exception as e:
        print(f"Oh no, we encountered {e}")
        raise


def foo():
    """Foo.

    Returns:
        42: int.
    """
    if True:
        raise TypeError  # DOC501
    else:
        raise TypeError  # no DOC501 here because we already emitted a diagnostic for the earlier `raise TypeError`
    raise ValueError  # DOC501
    return 42


# OK - exception raised via classmethod, documented in Raises section
def validate_classmethod(value):
    """Validate a value.

    Args:
        value: the value to validate.

    Raises:
        ValueError: if the value is invalid.
    """
    raise ValueError.from_exception_data(
        title="Validation",
        line_errors=[],
    )


# DOC501 - exception raised via classmethod, NOT documented
def validate_classmethod_missing(value):
    """Validate a value.

    Args:
        value: the value to validate.
    """
    raise ValueError.from_exception_data(
        title="Validation",
        line_errors=[],
    )


# OK - exception raised via classmethod using imported exception
def validate_imported_classmethod(value):
    """Validate a value.

    Args:
        value: the value to validate.

    Raises:
        AnotherError: if the value is invalid.
    """
    raise AnotherError.create(value)


# OK - chained method call with .with_traceback(), documented
def chained_with_traceback(value):
    """Validate a value.

    Args:
        value: the value to validate.

    Raises:
        ValueError: if the value is invalid.
    """
    raise ValueError.from_exception_data(
        title="Validation",
        line_errors=[],
    ).with_traceback(None)


# DOC501 - chained method call, NOT documented
def chained_missing(value):
    """Validate a value.

    Args:
        value: the value to validate.
    """
    raise ValueError.from_exception_data(
        title="Validation",
        line_errors=[],
    ).with_traceback(None)
