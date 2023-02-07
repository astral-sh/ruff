"""
Should emit:
B011 - on line 8
B011 - on line 10
"""

assert 1 != 2
assert False
assert 1 != 2, "message"
assert False, "message"
