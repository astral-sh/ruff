import paramiko

ssh = paramiko.SSHClient()
ssh.exec_command('something; really; unsafe')