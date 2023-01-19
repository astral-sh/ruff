"""Check that magic values are not used in comparisons."""

user_input = 10

if 10 > user_input:  # [magic-value-comparison]
    pass

if 10 == 100:  # [comparison-of-constants] R0133
    pass

if 1 == 3:  # [comparison-of-constants] R0133
    pass

x = 0
if 4 == 3 == x:  # [comparison-of-constants] R0133
    pass

time_delta = 7224
ONE_HOUR = 3600

if time_delta > ONE_HOUR:  # correct
    pass

argc = 1

if argc != -1:  # correct
    pass

if argc != 0:  # correct
    pass

if argc != 1:  # correct
    pass

if argc != 2:  # [magic-value-comparison]
    pass

if __name__ == "__main__":  # correct
    pass

ADMIN_PASSWORD = "SUPERSECRET"
input_password = "password"

if input_password == "":  # correct
    pass

if input_password == ADMIN_PASSWORD:  # correct
    pass

if input_password == "Hunter2":  # [magic-value-comparison]
    pass

PI = 3.141592653589793238
pi_estimation = 3.14

if pi_estimation == 3.141592653589793238:  # [magic-value-comparison]
    pass

if pi_estimation == PI:  # correct
    pass

HELLO_WORLD = b"Hello, World!"
user_input = b"Hello, There!"

if user_input == b"something":  # [magic-value-comparison]
    pass

if user_input == HELLO_WORLD:  # correct
    pass
