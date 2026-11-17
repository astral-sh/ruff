import paramiko

paramiko.exec_command('something; really; unsafe')

# Errors (preview)
client = paramiko.SSHClient()
client.exec_command("echo $HOME")

paramiko.SSHClient().exec_command("ls")

annotated: paramiko.SSHClient = make_client()
annotated.exec_command("ls")

with paramiko.SSHClient() as managed:
    managed.exec_command("ls")


def run(ssh: paramiko.SSHClient, channel: paramiko.Channel):
    ssh.exec_command("ls")
    channel.exec_command("ls")


from paramiko.client import SSHClient
from paramiko.channel import Channel

SSHClient().exec_command("ls")
Channel(1).exec_command("ls")

# OK
client.connect("example.com")
other = make_client()
other.exec_command("ls")
exec_command("ls")

reassigned = paramiko.SSHClient()
reassigned = make_client()
reassigned.exec_command("ls")
