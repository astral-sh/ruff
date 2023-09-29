import paramiko
from paramiko import client
from paramiko.client import AutoAddPolicy, WarningPolicy

ssh_client = client.SSHClient()
ssh_client_from_paramiko = paramiko.SSHClient()

# OK
ssh_client.set_missing_host_key_policy(policy=foo)
ssh_client.set_missing_host_key_policy(client.MissingHostKeyPolicy)
ssh_client.set_missing_host_key_policy()
ssh_client.set_missing_host_key_policy(foo)

# Errors
ssh_client.set_missing_host_key_policy(client.AutoAddPolicy)
ssh_client.set_missing_host_key_policy(client.WarningPolicy)
ssh_client.set_missing_host_key_policy(client.AutoAddPolicy())
ssh_client.set_missing_host_key_policy(AutoAddPolicy)
ssh_client.set_missing_host_key_policy(policy=client.AutoAddPolicy)
ssh_client.set_missing_host_key_policy(policy=client.WarningPolicy)
ssh_client.set_missing_host_key_policy(policy=WarningPolicy)
ssh_client_from_paramiko.set_missing_host_key_policy(paramiko.AutoAddPolicy)

# Unrelated
set_missing_host_key_policy(client.AutoAddPolicy)
foo.set_missing_host_key_policy(client.AutoAddPolicy)
ssh_client = 1
ssh_client.set_missing_host_key_policy(client.AutoAddPolicy)
