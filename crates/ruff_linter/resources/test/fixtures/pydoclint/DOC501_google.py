from somewhere import AnotherError


class FasterThanLightError(Exception):
    ...


_some_error = Exception


# DOC501
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
