import datetime

# no args
datetime.datetime.now()

# wrong keywords
datetime.datetime.now(bad=datetime.timezone.utc)

# none args
datetime.datetime.now(None)

# none keywords
datetime.datetime.now(tz=None)

from datetime import datetime

# no args unqualified
datetime.now()
