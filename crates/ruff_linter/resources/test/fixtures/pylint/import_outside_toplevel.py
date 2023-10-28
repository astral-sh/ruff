def print_python_version():
    import sys  # C0415

    print(sys.version_info)

def print_python_version():
    import sys, string  # C0415

    print(sys.version_info, string.ascii.digits)

# OK
import sys 


def print_python_version():
    print(sys.version_info)

# OK
if True:
    import sys