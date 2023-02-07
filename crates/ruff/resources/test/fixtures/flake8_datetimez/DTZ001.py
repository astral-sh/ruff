import datetime

# no args
datetime.datetime(2000, 1, 1, 0, 0, 0)

# none args
datetime.datetime(2000, 1, 1, 0, 0, 0, 0, None)

# not none arg
datetime.datetime(2000, 1, 1, 0, 0, 0, 0, datetime.timezone.utc)

# no kwargs
datetime.datetime(2000, 1, 1, fold=1)

# none kwargs
datetime.datetime(2000, 1, 1, tzinfo=None)

from datetime import datetime

# no args unqualified
datetime(2000, 1, 1, 0, 0, 0)
