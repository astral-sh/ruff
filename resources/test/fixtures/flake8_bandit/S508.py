from pysnmp.hlapi import CommunityData

CommunityData("public", mpModel=0)  # S508
CommunityData("public", mpModel=1)  # S508

CommunityData("public", mpModel=2)  # OK
