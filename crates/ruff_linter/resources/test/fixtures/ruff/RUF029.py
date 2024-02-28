a is b is c  # OK
a == b == c  # OK
a < b < c  # OK
a <= b <= c  # OK
a < b <= c  # OK
a <= b < c  # OK
a > b > c  # OK
a >= b >= c  # OK
a > b >= c  # OK
a >= b > c  # OK
a is not b  # OK
a != b  # OK
a is b is not c  # RUF029
a is not b is c  # RUF029
a is b == c  # RUF029
a == b is c  # RUF029
a is b < c  # RUF029
a < b is c  # RUF029
a > b < c  # RUF029
a < b > c  # RUF029
