"""
   Author
"""


class Platform:
    """ Remove sampler
            Args:
            Returns:
            """


def memory_test():
    """
　　　参数含义：precision:精确到小数点后几位
    """



class Platform:
    """Over indented last line
    Args:
    Returns:
                """


class Platform:
    """All lines are over indented including the last containing the closing quotes
        Args:
        Returns:
        """

class Platform:
    """All lines are over indented including the last
        Args:
        Returns"""

# OK: This doesn't get flagged because it is accepted when the closing quotes are on a separate line  (see next test). Raises D209
class Platform:
    """Over indented last line with content
    Args:
        Some content on the last line"""

# OK:
class Platform:
    """Over indented last line with content
    Args:
        Some content on the last line
    """
