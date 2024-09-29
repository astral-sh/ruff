# Errors

_ = "0123456789"
_ = "01234567"
_ = "0123456789abcdefABCDEF"
_ = "abcdefghijklmnopqrstuvwxyz"
_ = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
_ = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
_ = r"""!"#$%&'()*+,-./:;<=>?@[\]^_`{|}~"""
_ = " \t\n\r\v\f"

_ = "" in "1234567890"
_ = "" in "12345670"
_ = '0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!"#$%&\'()*+,-./:;<=>?@[\\]^_`{|}~ \t\n\r\x0b\x0c'
_ = (
    '0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!"#$%&'
    "'()*+,-./:;<=>?@[\\]^_`{|}~ \t\n\r\x0b\x0c"
)
_ = id("0123"
       "4567"
       "89")
_ = "" in ("123"
           "456"
           "789"
           "0")

# Ok

_ = "1234567890"
_ = "1234"
_ = "" in "1234"
