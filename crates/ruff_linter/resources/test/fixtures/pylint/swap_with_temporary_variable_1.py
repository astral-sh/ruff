"""
Test that PLR1712 fires in the module-global scope.
"""

x, y = 1, 2
temp = x  # PLR1712
x = y
y = temp
