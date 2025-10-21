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


# This should NOT trigger DOC502 because OSError is explicitly re-raised
def f():
    """Do nothing.

    Raises
    ------
    OSError
        If the OS errors.
    """
    try:
        pass
    except OSError as e:
        raise e


# This should NOT trigger DOC502 because OSError is explicitly re-raised with from None
def g():
    """Do nothing.

    Raises
    ------
    OSError
        If the OS errors.
    """
    try:
        pass
    except OSError as e:
        raise e from None
