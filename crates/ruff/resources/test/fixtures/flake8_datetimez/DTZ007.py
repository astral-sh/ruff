import datetime

# bad format
datetime.datetime.strptime("something", "%H:%M:%S%Z")

# no replace or astimezone
datetime.datetime.strptime("something", "something")

# wrong replace
datetime.datetime.strptime("something", "something").replace(hour=1)

# none replace
datetime.datetime.strptime("something", "something").replace(tzinfo=None)

# OK
datetime.datetime.strptime("something", "something").replace(
    tzinfo=datetime.timezone.utc
)

# OK
datetime.datetime.strptime("something", "something").astimezone()

# OK
datetime.datetime.strptime("something", "%H:%M:%S%z")

# OK
datetime.datetime.strptime("something", something).astimezone()

# OK
datetime.datetime.strptime("something", something).replace(tzinfo=datetime.timezone.utc)

from datetime import datetime

# no replace orastimezone unqualified
datetime.strptime("something", "something")
