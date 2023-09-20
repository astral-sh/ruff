"""Check that magic values are not used in comparisons"""

if 100 == 100: # [comparison-of-constants]
    pass

if 1 == 3: # [comparison-of-constants]
    pass

if 1 != 3: # [comparison-of-constants]
    pass

x = 0
if 4 == 3 == x: # [comparison-of-constants]
    pass

if x == 0: # correct
    pass

y = 1
if x == y: # correct
    pass

if 1 > 0: # [comparison-of-constants]
    pass

if x > 0: # correct
    pass

if 1 >= 0: # [comparison-of-constants]
    pass

if x >= 0: # correct
    pass

if 1 < 0: # [comparison-of-constants]
    pass

if x < 0: # correct
    pass

if 1 <= 0: # [comparison-of-constants]
    pass

if x <= 0: # correct
    pass

word = "hello"
if word == "": # correct
    pass

if "hello" == "": # [comparison-of-constants]
    pass

truthy = True
if truthy == True: # correct
    pass

if True == False: # [comparison-of-constants]
    pass
