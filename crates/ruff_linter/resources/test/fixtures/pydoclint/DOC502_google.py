class FasterThanLightError(Exception):
    ...


# DOC502
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
    return distance / time


# DOC502
def calculate_speed(distance: float, time: float) -> float:
    """Calculate speed as distance divided by time.

    Args:
        distance: Distance traveled.
        time: Time spent traveling.

    Returns:
        Speed as distance divided by time.

    Raises:
        FasterThanLightError: If speed is greater than the speed of light.
        DivisionByZero: Divide by zero.
    """
    return distance / time


# DOC502
def calculate_speed(distance: float, time: float) -> float:
    """Calculate speed as distance divided by time.

    Args:
        distance: Distance traveled.
        time: Time spent traveling.

    Returns:
        Speed as distance divided by time.

    Raises:
        FasterThanLightError: If speed is greater than the speed of light.
        DivisionByZero: Divide by zero.
    """
    try:
        return distance / time
    except ZeroDivisionError as exc:
        raise FasterThanLightError from exc


# This is fine
def calculate_speed(distance: float, time: float) -> float:
    """Calculate speed as distance divided by time.

    Args:
        distance: Distance traveled.
        time: Time spent traveling.

    Returns:
        Speed as distance divided by time.

    Raises:
        ZeroDivisionError: If you pass `0` for the time
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


# DOC502 regression for Sphinx directive after Raises (issue #18959)
def foo():
    """First line.

    Raises:
        ValueError:
            some text

    .. versionadded:: 0.7.0
        The ``init_kwargs`` argument.
    """
    raise ValueError


# DOC502 regression for following section with colons
def example_with_following_section():
    """Summary.
    
    Returns:
        str: The resulting expression.
    
    Raises:
        ValueError: If the unit is not valid.
    
    Relation to `time_range_lookup`:
        - Handles the "start of" modifier.
        - Example: "start of month" → `DATETRUNC()`.
    """
    raise ValueError


# This should NOT trigger DOC502 because OSError is explicitly re-raised
def f():
    """Do nothing.

    Raises:
        OSError: If the OS errors.
    """
    try:
        pass
    except OSError as e:
        raise e


# This should NOT trigger DOC502 because OSError is explicitly re-raised with from None
def g():
    """Do nothing.

    Raises:
        OSError: If the OS errors.
    """
    try:
        pass
    except OSError as e:
        raise e from None


# This should NOT trigger DOC502 because ValueError is explicitly re-raised from tuple exception
def h():
    """Do nothing.

    Raises:
        ValueError: If something goes wrong.
    """
    try:
        pass
    except (ValueError, TypeError) as e:
        raise e


# This should NOT trigger DOC502 because TypeError is explicitly re-raised from tuple exception
def i():
    """Do nothing.

    Raises:
        TypeError: If something goes wrong.
    """
    try:
        pass
    except (ValueError, TypeError) as e:
        raise e
