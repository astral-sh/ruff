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

# uses `astimezone` method
datetime.now().astimezone()
datetime.now().astimezone


# https://github.com/astral-sh/ruff/issues/15998

## Errors
datetime.now().replace.astimezone()
datetime.now().replace[0].astimezone()
datetime.now()().astimezone()
datetime.now().replace(datetime.now()).astimezone()

foo.replace(datetime.now().replace).astimezone()

## No errors
datetime.now().replace(microsecond=0).astimezone()
datetime.now().replace(0).astimezone()
datetime.now().replace(0).astimezone
datetime.now().replace(0).replace(1).astimezone
