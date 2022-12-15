import datetime


# bad format
datetime.datetime.strptime('something', "%H:%M:%S%Z")

# no replace or astimezone
datetime.datetime.strptime('something', 'something')

# no replace orastimezone unqualified
datetime.strptime('something', 'something')

# wrong replace
datetime.datetime.strptime('something', 'something').replace(hour=1)

# none replace
datetime.datetime.strptime('something', 'something').replace(tzinfo=None)
