import datetime


# no args
datetime.datetime.fromtimestamp(1234)

# no args unqualified
datetime.fromtimestamp(1234)

# wrong keywords
datetime.datetime.fromtimestamp(1234, bad=datetime.timezone.utc)

# none args
datetime.datetime.fromtimestamp(1234, None)

# none keywords
datetime.datetime.fromtimestamp(1234, tz=None)
