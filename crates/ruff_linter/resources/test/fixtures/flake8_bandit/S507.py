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


# Errors: subclasses of `SSHClient` defined in the same file (#21407)
class CustomClient(paramiko.SSHClient):
    def __init__(self):
        super().__init__()
        self.set_missing_host_key_policy(paramiko.AutoAddPolicy)


class NestedCustomClient(CustomClient):
    def connect_insecurely(self):
        self.set_missing_host_key_policy(client.WarningPolicy())


custom_client = CustomClient()
custom_client.set_missing_host_key_policy(paramiko.AutoAddPolicy)
nested_custom_client = NestedCustomClient()
nested_custom_client.set_missing_host_key_policy(AutoAddPolicy)


# OK: not a subclass of `SSHClient`, or not an instance method receiver
class NotAClient:
    def __init__(self):
        self.set_missing_host_key_policy(paramiko.AutoAddPolicy)


not_a_client = NotAClient()
not_a_client.set_missing_host_key_policy(paramiko.AutoAddPolicy)


class StaticClient(paramiko.SSHClient):
    @staticmethod
    def configure(other):
        other.set_missing_host_key_policy(paramiko.AutoAddPolicy)


def configure(self):
    self.set_missing_host_key_policy(paramiko.AutoAddPolicy)
