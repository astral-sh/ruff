from pysnmp.hlapi import UsmUserData


insecure = UsmUserData("securityName")  # S509
auth_no_priv = UsmUserData("securityName", "authName")  # S509

less_insecure = UsmUserData("securityName", "authName", "privName")  # OK
