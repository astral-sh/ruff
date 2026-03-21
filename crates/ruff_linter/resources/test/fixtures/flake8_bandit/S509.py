from pysnmp.hlapi import UsmUserData


insecure = UsmUserData("securityName")  # S509
auth_no_priv = UsmUserData("securityName", "authName")  # S509

less_insecure = UsmUserData("securityName", "authName", "privName")  # OK

# New API paths
import pysnmp.hlapi.asyncio
import pysnmp.hlapi.v3arch.asyncio
import pysnmp.hlapi.v3arch.asyncio.auth
import pysnmp.hlapi.auth

pysnmp.hlapi.asyncio.UsmUserData("user")  # S509
pysnmp.hlapi.v3arch.asyncio.UsmUserData("user")  # S509
pysnmp.hlapi.v3arch.asyncio.auth.UsmUserData("user")  # S509
pysnmp.hlapi.auth.UsmUserData("user")  # S509

pysnmp.hlapi.asyncio.UsmUserData("user", "authkey", "privkey")  # OK
pysnmp.hlapi.v3arch.asyncio.UsmUserData("user", "authkey", "privkey")  # OK
pysnmp.hlapi.v3arch.asyncio.auth.UsmUserData("user", "authkey", "privkey")  # OK
pysnmp.hlapi.auth.UsmUserData("user", "authkey", "privkey")  # OK
