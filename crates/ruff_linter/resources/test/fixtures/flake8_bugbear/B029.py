"""
Should emit:
B029 - on lines 8, 13, 18 and 23
"""

try:
    pass
except ():
    pass

try:
    pass
except () as e:
    pass

try:
    pass
except* ():
    pass

try:
    pass
except* () as e:
    pass
