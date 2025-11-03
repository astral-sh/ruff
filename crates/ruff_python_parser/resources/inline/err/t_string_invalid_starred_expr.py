# parse_options: {"target-version": "3.14"}
# Starred expression inside t-string has a minimum precedence of bitwise or.
t"{*}"
t"{*x and y}"
t"{*yield x}"
