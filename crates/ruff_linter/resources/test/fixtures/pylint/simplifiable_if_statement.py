x = 2

if x == 1:
    is_one = True
else:
    is_one = False  # PLR1703

if x == 2:
    isnt_two = False
else:
    isnt_two = True  # PLR1703

if( x
    ==
      2):
    isnt_two = False
else:
    isnt_two = True  # PLR1703


# OK
if x == 1:
    is_one = True
else:
    is_one = True  # both True, doesn't emit

# OK
if x == 1:
    is_one = False
else:
    is_one = False  # both False, doesn't emit
