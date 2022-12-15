"""
 Test that shadowing a global with a class attribute does not produce a
 warning.
 """

import fu


class bar:
    fu = 1


print(fu)
