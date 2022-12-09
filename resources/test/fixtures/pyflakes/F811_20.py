"""
 Test that shadowing a global with a class attribute does not produce a
 warning.
 """

import fu


class bar:
    # STOPSHIP: This errors.
    fu = 1


print(fu)
