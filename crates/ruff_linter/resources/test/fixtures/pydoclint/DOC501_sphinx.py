class FasterThanLightError(Exception):
    ...


# OK
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
    try:
        return distance / time
    except ZeroDivisionError as exc:
        raise FasterThanLightError from exc


# DOC501
def calculate_speed(distance: float, time: float) -> float:
    """
    Calculate speed as distance divided by time.

    :param distance: Distance traveled.
    :type distance: float
    :param time: Time spent traveling.
    :type time: float
    :return: Speed as distance divided by time.
    :rtype: float
    """
    try:
        return distance / time
    except ZeroDivisionError as exc:
        raise FasterThanLightError from exc


# DOC501
def calculate_speed(distance: float, time: float) -> float:
    """
    Calculate speed as distance divided by time.

    :param distance: Distance traveled.
    :type distance: float
    :param time: Time spent traveling.
    :type time: float
    :return: Speed as distance divided by time.
    :rtype: float
    """
    try:
        return distance / time
    except ZeroDivisionError as exc:
        raise FasterThanLightError from exc
    except:
        raise ValueError


# DOC501
def calculate_speed(distance: float, time: float) -> float:
    """
    Calculate speed as distance divided by time.

    :param distance: Distance traveled.
    :type distance: float
    :param time: Time spent traveling.
    :type time: float
    :return: Speed as distance divided by time.
    :rtype: float
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


# This is fine
def calculate_speed(distance: float, time: float) -> float:
    """
    Calculate speed as distance divided by time.

    :param distance: Distance traveled.
    :type distance: float
    :param time: Time spent traveling.
    :type time: float
    :return: Speed as distance divided by time.
    :rtype: float
    """
    try:
        return distance / time
    except Exception as e:
        print(f"Oh no, we encountered {e}")
        raise


def foo():
    """Foo.

    :return: 42
    :rtype: int
    """
    if True:
        raise TypeError  # DOC501
    else:
        raise TypeError  # no DOC501 here because we already emitted a diagnostic for the earlier `raise TypeError`
    raise ValueError  # DOC501
    return 42
