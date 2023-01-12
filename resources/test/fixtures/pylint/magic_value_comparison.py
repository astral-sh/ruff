"""Check that magic values are not used in comparisons"""

user_input = 10

if 10 > user_input: # [magic-value-comparison]
    pass

if 10 == 100: # [magic-value-comparison]
    pass

time_delta = 7224
one_hour = 3600

if time_delta > one_hour: # correct
    pass

argc = 1

if argc != 1: # correct
    pass

