#!/usr/bin/python

if __name__ == '__main__':
    #EXE002 Checks for executable .py files that do not have a shebang.
    executable = True
    shebang = True
    rule_should = "pass"
    print(f'EXE002 should {rule_should}: Executable: {executable}, shebang present: {shebang}')
