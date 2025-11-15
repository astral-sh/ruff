from pysnmp.hlapi import CommunityData

CommunityData("public", mpModel=0)  # S508
CommunityData("public", mpModel=1)  # S508

CommunityData("public", mpModel=2)  # OK

# New API paths
import pysnmp.hlapi.asyncio
import pysnmp.hlapi.v1arch
import pysnmp.hlapi.v1arch.asyncio
import pysnmp.hlapi.v1arch.asyncio.auth
import pysnmp.hlapi.v3arch
import pysnmp.hlapi.v3arch.asyncio
import pysnmp.hlapi.v3arch.asyncio.auth
import pysnmp.hlapi.auth

pysnmp.hlapi.asyncio.CommunityData("public", mpModel=0)  # S508
pysnmp.hlapi.v1arch.asyncio.auth.CommunityData("public", mpModel=0)  # S508
pysnmp.hlapi.v1arch.asyncio.CommunityData("public", mpModel=0)  # S508
pysnmp.hlapi.v1arch.CommunityData("public", mpModel=0)  # S508
pysnmp.hlapi.v3arch.asyncio.auth.CommunityData("public", mpModel=0)  # S508
pysnmp.hlapi.v3arch.asyncio.CommunityData("public", mpModel=0)  # S508
pysnmp.hlapi.v3arch.CommunityData("public", mpModel=0)  # S508
pysnmp.hlapi.auth.CommunityData("public", mpModel=0)  # S508

pysnmp.hlapi.asyncio.CommunityData("public", mpModel=2)  # OK
pysnmp.hlapi.v1arch.asyncio.auth.CommunityData("public", mpModel=2)  # OK
pysnmp.hlapi.v1arch.asyncio.CommunityData("public", mpModel=2)  # OK
pysnmp.hlapi.v1arch.CommunityData("public", mpModel=2)  # OK
pysnmp.hlapi.v3arch.asyncio.auth.CommunityData("public", mpModel=2)  # OK
pysnmp.hlapi.v3arch.asyncio.CommunityData("public", mpModel=2)  # OK
pysnmp.hlapi.v3arch.CommunityData("public", mpModel=2)  # OK
pysnmp.hlapi.auth.CommunityData("public", mpModel=2)  # OK
