class FasterThanLightError(Exception):
    ...


# DOC502
def calculate_speed(distance: float, time: float) -> float:
    """
    Calculate speed as distance divided by time.

    :param distance: Distance traveled.
    :type distance: float
    :param time: Time spent traveling.
    :type time: float
    :return: Speed as distance divided by time.
    :rtype: float
    :raises FasterThanLightError: If speed is greater than the speed of light.
    """
    return distance / time


# DOC502
def calculate_speed(distance: float, time: float) -> float:
    """
    Calculate speed as distance divided by time.

    :param distance: Distance traveled.
    :type distance: float
    :param time: Time spent traveling.
    :type time: float
    :return: Speed as distance divided by time.
    :rtype: float
    :raises FasterThanLightError: If speed is greater than the speed of light.
    :raises DivisionByZero: If attempting to divide by zero.
    """
    return distance / time


# DOC502
def calculate_speed(distance: float, time: float) -> float:
    """
    Calculate speed as distance divided by time.

    :param distance: Distance traveled.
    :type distance: float
    :param time: Time spent traveling.
    :type time: float
    :return: Speed as distance divided by time.
    :rtype: float
    :raises FasterThanLightError: If speed is greater than the speed of light.
    :raises DivisionByZero: If attempting to divide by zero.
    """
    try:
        return distance / time
    except ZeroDivisionError as exc:
        raise FasterThanLightError from exc


# This is fine
def calculate_speed(distance: float, time: float) -> float:
    """Calculate speed as distance divided by time.

    Calculate speed as distance divided by time.

    :param distance: Distance traveled.
    :type distance: float
    :param time: Time spent traveling.
    :type time: float
    :return: Speed as distance divided by time.
    :rtype: float
    :raises TypeError: If one or both of the parameters is not a number.
    :raises ZeroDivisionError: If attempting to divide by zero.
    """
    try:
        return distance / time
    except ZeroDivisionError:
        print("Oh no, why would you divide something by zero?")
        raise
    except TypeError:
        print("Not a number? Shame on you!")
        raise
