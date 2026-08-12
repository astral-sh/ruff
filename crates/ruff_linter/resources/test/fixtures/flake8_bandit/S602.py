from subprocess import Popen, call, check_call, check_output, run

# Check different Popen wrappers are checked.
Popen("true", shell=True)
call("true", shell=True)
check_call("true", shell=True)
check_output("true", shell=True)
run("true", shell=True)

# https://github.com/astral-sh/ruff/issues/27631
# Keyword command arguments are checked too.
Popen(args="true", shell=True)
call(args="true", shell=True)
check_call(args="true", shell=True)
check_output(args="true", shell=True)
run(args="true", shell=True)

# Check values that truthy values are treated as true.
Popen("true", shell=1)
Popen("true", shell=[1])
Popen("true", shell={1: 1})
Popen("true", shell=(1,))

# Check command argument looks unsafe.
var_string = "true"
Popen(var_string, shell=True)
Popen([var_string], shell=True)
Popen([var_string, ""], shell=True)
Popen(args=var_string, shell=True)

# Starred command arguments are untrusted.
cmd = input()
Popen(*cmd, shell=True)

# Check dict display with only double-starred expressions can be falsey.
Popen("true", shell={**{}})
Popen("true", shell={**{**{}}})

# Check pattern with merged defaults/configs
class ShellConfig:
    def __init__(self):
        self.shell_defaults = {}

    def fetch_shell_config(self, username):
        return {}

    def run(self, username):
        Popen("true", shell={**self.shell_defaults, **self.fetch_shell_config(username)})

# Additional truthiness cases for generator, lambda, and f-strings
Popen("true", shell=(i for i in ()))
Popen("true", shell=lambda: 0)
Popen("true", shell=f"{b''}")
x = 1
Popen("true", shell=f"{x=}")
