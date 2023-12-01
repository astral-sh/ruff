s = "qwe"
s.strip(s)  # no warning
s.strip("we")  # no warning
s.strip(".facebook.com")  # warning
s.strip("e")  # no warning
s.strip("\n\t ")  # no warning
s.strip(r"\n\t ")  # warning
s.lstrip(s)  # no warning
s.lstrip("we")  # no warning
s.lstrip(".facebook.com")  # warning
s.lstrip("e")  # no warning
s.lstrip("\n\t ")  # no warning
s.lstrip(r"\n\t ")  # warning
s.rstrip(s)  # no warning
s.rstrip("we")  # warning
s.rstrip(".facebook.com")  # warning
s.rstrip("e")  # no warning
s.rstrip("\n\t ")  # no warning
s.rstrip(r"\n\t ")  # warning
s.strip("a")  # no warning
s.strip("あ")  # no warning
s.strip("ああ")  # warning
s.strip("\ufeff")  # no warning
s.strip("\u0074\u0065\u0073\u0074")  # warning

from somewhere import other_type, strip

strip("we")  # no warning
other_type().lstrip()  # no warning
other_type().rstrip(["a", "b", "c"])  # no warning
other_type().strip("a", "b")  # no warning
