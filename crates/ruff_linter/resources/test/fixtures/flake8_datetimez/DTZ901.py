import datetime


# Error
datetime.datetime.max
datetime.datetime.min

datetime.datetime.max.replace(year=...)
datetime.datetime.min.replace(hour=...)


# No error
datetime.datetime.max.replace(tzinfo=...)
datetime.datetime.min.replace(tzinfo=...)

datetime.datetime.max.time()
datetime.datetime.min.time()

datetime.datetime.max.time(foo=...)
datetime.datetime.min.time(foo=...)


from datetime import datetime


# Error
datetime.max
datetime.min

datetime.max.replace(year=...)
datetime.min.replace(hour=...)


# No error
datetime.max.replace(tzinfo=...)
datetime.min.replace(tzinfo=...)

datetime.max.time()
datetime.min.time()

datetime.max.time(foo=...)
datetime.min.time(foo=...)
