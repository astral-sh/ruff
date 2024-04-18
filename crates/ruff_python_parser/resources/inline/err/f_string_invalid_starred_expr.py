# Starred expression inside f-string has a minimum precedence of bitwise or.
f"{*}"
f"{*x and y}"
f"{*yield x}"
