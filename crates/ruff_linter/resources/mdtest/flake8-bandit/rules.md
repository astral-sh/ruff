# `flake8-bandit`

Regression tests for <https://github.com/astral-sh/ruff/issues/27631>. Keyword command arguments are
checked, as well as positional arguments. `*args` is also treated as untrusted.

## `subprocess-popen-with-shell-equals-true` (`S602`)

```toml
[lint]
select = ["S602"]
```

```py
from subprocess import Popen, call, check_call, check_output, run

Popen(args="true", shell=True)  # error: [subprocess-popen-with-shell-equals-true]
call(args="true", shell=True)  # error: [subprocess-popen-with-shell-equals-true]
check_call(args="true", shell=True)  # error: [subprocess-popen-with-shell-equals-true]
check_output(args="true", shell=True)  # error: [subprocess-popen-with-shell-equals-true]
run(args="true", shell=True)  # error: [subprocess-popen-with-shell-equals-true]

var_string = "true"
Popen(args=var_string, shell=True)  # error: [subprocess-popen-with-shell-equals-true]

cmd = input()
Popen(*cmd, shell=True)  # error: [subprocess-popen-with-shell-equals-true]
```

## `subprocess-without-shell-equals-true` (`S603`)

```toml
[lint]
select = ["S603"]
```

```py
from subprocess import Popen, call, check_call, check_output, run

a = input()

Popen(args=a, shell=False)  # error: [subprocess-without-shell-equals-true]
call(args=a, shell=False)  # error: [subprocess-without-shell-equals-true]
check_call(args=a, shell=False)  # error: [subprocess-without-shell-equals-true]
check_output(args=a, shell=False)  # error: [subprocess-without-shell-equals-true]
run(args=a, shell=False)  # error: [subprocess-without-shell-equals-true]
check_output(args=[a], shell=False)  # error: [subprocess-without-shell-equals-true]
run(*a)  # error: [subprocess-without-shell-equals-true]
run(args=["true"])
```

## `start-process-with-partial-path` (`S607`)

```toml
[lint]
select = ["S607"]
```

```py
import os
import subprocess

os.spawnv(mode=os.P_WAIT, file="/bin/ls", args=["ls"])
subprocess.run(args="git status")  # error: [start-process-with-partial-path]
```

## `unix-command-wildcard-injection` (`S609`)

```toml
[lint]
select = ["S609"]
```

```py
import subprocess

subprocess.Popen(args="chmod -R 777 *", shell=True)  # error: [unix-command-wildcard-injection]
```
