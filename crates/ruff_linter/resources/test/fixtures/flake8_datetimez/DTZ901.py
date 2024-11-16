import datetime


# Error
datetime.datetime.max
datetime.datetime.min

datetime.datetime.max.replace(year=...)
datetime.datetime.min.replace(hour=...)


# No error
datetime.datetime.max.replace(tzinfo=...)
datetime.datetime.min.replace(tzinfo=...)


from datetime import datetime


# Error
datetime.max
datetime.min

datetime.max.replace(year=...)
datetime.min.replace(hour=...)


# No error
datetime.max.replace(tzinfo=...)
datetime.min.replace(tzinfo=...)
