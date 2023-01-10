from pysnmp.hlapi import UsmUserData

# SHOULD FAIL
insecure = UsmUserData("securityName")
auth_no_priv = UsmUserData("securityName","authName")
# SHOULD PASS
less_insecure = UsmUserData("securityName","authName","privName")
