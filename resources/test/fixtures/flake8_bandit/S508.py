from pysnmp.hlapi import CommunityData

# SHOULD FAIL
a = CommunityData('public', mpModel=0)
a = CommunityData('public', mpModel=1)
# SHOULD PASS
a = CommunityData('public', mpModel=2)
