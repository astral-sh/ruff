class FasterThanLightError(Exception):
    ...


# DOC502
def calculate_speed(distance: float, time: float) -> float:
    """
    Calculate speed as distance divided by time.

    Parameters
    ----------
    distance : float
        Distance traveled.
    time : float
        Time spent traveling.

    Returns
    -------
    float
        Speed as distance divided by time.

    Raises
    ------
    FasterThanLightError
        If speed is greater than the speed of light.
    """
    return distance / time


# DOC502
def calculate_speed(distance: float, time: float) -> float:
    """
    Calculate speed as distance divided by time.

    Parameters
    ----------
    distance : float
        Distance traveled.
    time : float
        Time spent traveling.

    Returns
    -------
    float
        Speed as distance divided by time.

    Raises
    ------
    FasterThanLightError
        If speed is greater than the speed of light.
    DivisionByZero
        If attempting to divide by zero.
    """
    return distance / time


# DOC502
def calculate_speed(distance: float, time: float) -> float:
    """
    Calculate speed as distance divided by time.

    Parameters
    ----------
    distance : float
        Distance traveled.
    time : float
        Time spent traveling.

    Returns
    -------
    float
        Speed as distance divided by time.

    Raises
    ------
    FasterThanLightError
        If speed is greater than the speed of light.
    DivisionByZero
        If attempting to divide by zero.
    """
    try:
        return distance / time
    except ZeroDivisionError as exc:
        raise FasterThanLightError from exc
