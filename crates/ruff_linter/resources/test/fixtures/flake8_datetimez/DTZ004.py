import datetime

# qualified
datetime.datetime.utcfromtimestamp(1234)

from datetime import datetime

# unqualified
datetime.utcfromtimestamp(1234)

# uses `astimezone` method
datetime.utcfromtimestamp(1234).astimezone()
