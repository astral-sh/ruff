import sys
from _typeshed import ReadableBuffer, WriteableBuffer
from collections.abc import Iterable
from socket import error as error, gaierror as gaierror, herror as herror, timeout as timeout
from typing import Any, Final, SupportsIndex, overload
from typing_extensions import CapsuleType, TypeAlias, disjoint_base

_CMSG: TypeAlias = tuple[int, int, bytes]
_CMSGArg: TypeAlias = tuple[int, int, ReadableBuffer]

# Addresses can be either tuples of varying lengths (AF_INET, AF_INET6,
# AF_NETLINK, AF_TIPC) or strings/buffers (AF_UNIX).
# See getsockaddrarg() in socketmodule.c.
_Address: TypeAlias = tuple[Any, ...] | str | ReadableBuffer
_RetAddress: TypeAlias = Any

# ===== Constants =====
# This matches the order in the CPython documentation
# https://docs.python.org/3/library/socket.html#constants

if sys.platform != "win32":
    AF_UNIX: Final[int]

AF_INET: Final[int]
AF_INET6: Final[int]

AF_UNSPEC: Final[int]

SOCK_STREAM: Final[int]
SOCK_DGRAM: Final[int]
SOCK_RAW: Final[int]
SOCK_RDM: Final[int]
SOCK_SEQPACKET: Final[int]

if sys.platform == "linux":
    # Availability: Linux >= 2.6.27
    SOCK_CLOEXEC: Final[int]
    SOCK_NONBLOCK: Final[int]

# --------------------
# Many constants of these forms, documented in the Unix documentation on
# sockets and/or the IP protocol, are also defined in the socket module.
# SO_*
# socket.SOMAXCONN
# MSG_*
# SOL_*
# SCM_*
# IPPROTO_*
# IPPORT_*
# INADDR_*
# IP_*
# IPV6_*
# EAI_*
# AI_*
# NI_*
# TCP_*
# --------------------

SO_ACCEPTCONN: Final[int]
SO_BROADCAST: Final[int]
SO_DEBUG: Final[int]
SO_DONTROUTE: Final[int]
SO_ERROR: Final[int]
SO_KEEPALIVE: Final[int]
SO_LINGER: Final[int]
SO_OOBINLINE: Final[int]
SO_RCVBUF: Final[int]
SO_RCVLOWAT: Final[int]
SO_RCVTIMEO: Final[int]
SO_REUSEADDR: Final[int]
SO_SNDBUF: Final[int]
SO_SNDLOWAT: Final[int]
SO_SNDTIMEO: Final[int]
SO_TYPE: Final[int]
if sys.platform != "linux":
    SO_USELOOPBACK: Final[int]
if sys.platform == "win32":
    SO_EXCLUSIVEADDRUSE: Final[int]
if sys.platform != "win32":
    SO_REUSEPORT: Final[int]
    if sys.platform != "darwin" or sys.version_info >= (3, 13):
        SO_BINDTODEVICE: Final[int]

if sys.platform != "win32" and sys.platform != "darwin":
    SO_DOMAIN: Final[int]
    SO_MARK: Final[int]
    SO_PASSCRED: Final[int]
    SO_PASSSEC: Final[int]
    SO_PEERCRED: Final[int]
    SO_PEERSEC: Final[int]
    SO_PRIORITY: Final[int]
    SO_PROTOCOL: Final[int]
if sys.platform != "win32" and sys.platform != "darwin" and sys.platform != "linux":
    SO_SETFIB: Final[int]
if sys.platform == "linux" and sys.version_info >= (3, 13):
    SO_BINDTOIFINDEX: Final[int]

SOMAXCONN: Final[int]

MSG_CTRUNC: Final[int]
MSG_DONTROUTE: Final[int]
MSG_OOB: Final[int]
MSG_PEEK: Final[int]
MSG_TRUNC: Final[int]
MSG_WAITALL: Final[int]
if sys.platform != "win32":
    MSG_DONTWAIT: Final[int]
    MSG_EOR: Final[int]
    MSG_NOSIGNAL: Final[int]  # Sometimes this exists on darwin, sometimes not
if sys.platform != "darwin":
    MSG_ERRQUEUE: Final[int]
if sys.platform == "win32":
    MSG_BCAST: Final[int]
    MSG_MCAST: Final[int]
if sys.platform != "win32" and sys.platform != "darwin":
    MSG_CMSG_CLOEXEC: Final[int]
    MSG_CONFIRM: Final[int]
    MSG_FASTOPEN: Final[int]
    MSG_MORE: Final[int]
if sys.platform != "win32" and sys.platform != "linux":
    MSG_EOF: Final[int]
if sys.platform != "win32" and sys.platform != "linux" and sys.platform != "darwin":
    MSG_NOTIFICATION: Final[int]
    MSG_BTAG: Final[int]  # Not FreeBSD either
    MSG_ETAG: Final[int]  # Not FreeBSD either

SOL_IP: Final[int]
SOL_SOCKET: Final[int]
SOL_TCP: Final[int]
SOL_UDP: Final[int]
if sys.platform != "win32" and sys.platform != "darwin":
    # Defined in socket.h for Linux, but these aren't always present for
    # some reason.
    SOL_ATALK: Final[int]
    SOL_AX25: Final[int]
    SOL_HCI: Final[int]
    SOL_IPX: Final[int]
    SOL_NETROM: Final[int]
    SOL_ROSE: Final[int]

if sys.platform != "win32":
    SCM_RIGHTS: Final[int]
if sys.platform != "win32" and sys.platform != "darwin":
    SCM_CREDENTIALS: Final[int]
if sys.platform != "win32" and sys.platform != "linux":
    SCM_CREDS: Final[int]

IPPROTO_ICMP: Final[int]
IPPROTO_IP: Final[int]
IPPROTO_RAW: Final[int]
IPPROTO_TCP: Final[int]
IPPROTO_UDP: Final[int]
IPPROTO_AH: Final[int]
IPPROTO_DSTOPTS: Final[int]
IPPROTO_EGP: Final[int]
IPPROTO_ESP: Final[int]
IPPROTO_FRAGMENT: Final[int]
IPPROTO_HOPOPTS: Final[int]
IPPROTO_ICMPV6: Final[int]
IPPROTO_IDP: Final[int]
IPPROTO_IGMP: Final[int]
IPPROTO_IPV6: Final[int]
IPPROTO_NONE: Final[int]
IPPROTO_PIM: Final[int]
IPPROTO_PUP: Final[int]
IPPROTO_ROUTING: Final[int]
IPPROTO_SCTP: Final[int]
if sys.platform != "linux":
    IPPROTO_GGP: Final[int]
    IPPROTO_IPV4: Final[int]
    IPPROTO_MAX: Final[int]
    IPPROTO_ND: Final[int]
if sys.platform == "win32":
    IPPROTO_CBT: Final[int]
    IPPROTO_ICLFXBM: Final[int]
    IPPROTO_IGP: Final[int]
    IPPROTO_L2TP: Final[int]
    IPPROTO_PGM: Final[int]
    IPPROTO_RDP: Final[int]
    IPPROTO_ST: Final[int]
if sys.platform != "win32":
    IPPROTO_GRE: Final[int]
    IPPROTO_IPIP: Final[int]
    IPPROTO_RSVP: Final[int]
    IPPROTO_TP: Final[int]
if sys.platform != "win32" and sys.platform != "linux":
    IPPROTO_EON: Final[int]
    IPPROTO_HELLO: Final[int]
    IPPROTO_IPCOMP: Final[int]
    IPPROTO_XTP: Final[int]
if sys.platform != "win32" and sys.platform != "darwin" and sys.platform != "linux":
    IPPROTO_BIP: Final[int]  # Not FreeBSD either
    IPPROTO_MOBILE: Final[int]  # Not FreeBSD either
    IPPROTO_VRRP: Final[int]  # Not FreeBSD either
if sys.platform == "linux":
    # Availability: Linux >= 2.6.20, FreeBSD >= 10.1
    IPPROTO_UDPLITE: Final[int]
if sys.version_info >= (3, 10) and sys.platform == "linux":
    IPPROTO_MPTCP: Final[int]

IPPORT_RESERVED: Final[int]
IPPORT_USERRESERVED: Final[int]

INADDR_ALLHOSTS_GROUP: Final[int]
INADDR_ANY: Final[int]
INADDR_BROADCAST: Final[int]
INADDR_LOOPBACK: Final[int]
INADDR_MAX_LOCAL_GROUP: Final[int]
INADDR_NONE: Final[int]
INADDR_UNSPEC_GROUP: Final[int]

IP_ADD_MEMBERSHIP: Final[int]
IP_DROP_MEMBERSHIP: Final[int]
IP_HDRINCL: Final[int]
IP_MULTICAST_IF: Final[int]
IP_MULTICAST_LOOP: Final[int]
IP_MULTICAST_TTL: Final[int]
IP_OPTIONS: Final[int]
if sys.platform != "linux":
    IP_RECVDSTADDR: Final[int]
if sys.version_info >= (3, 10):
    IP_RECVTOS: Final[int]
IP_TOS: Final[int]
IP_TTL: Final[int]
if sys.platform != "win32":
    IP_DEFAULT_MULTICAST_LOOP: Final[int]
    IP_DEFAULT_MULTICAST_TTL: Final[int]
    IP_MAX_MEMBERSHIPS: Final[int]
    IP_RECVOPTS: Final[int]
    IP_RECVRETOPTS: Final[int]
    IP_RETOPTS: Final[int]
if sys.version_info >= (3, 13) and sys.platform == "linux":
    CAN_RAW_ERR_FILTER: Final[int]
if sys.version_info >= (3, 14):
    IP_RECVTTL: Final[int]

    if sys.platform == "win32" or sys.platform == "linux":
        IPV6_RECVERR: Final[int]
        IP_RECVERR: Final[int]
        SO_ORIGINAL_DST: Final[int]

    if sys.platform == "win32":
        SOL_RFCOMM: Final[int]
        SO_BTH_ENCRYPT: Final[int]
        SO_BTH_MTU: Final[int]
        SO_BTH_MTU_MAX: Final[int]
        SO_BTH_MTU_MIN: Final[int]
        TCP_QUICKACK: Final[int]

    if sys.platform == "linux":
        IP_FREEBIND: Final[int]
        IP_RECVORIGDSTADDR: Final[int]
        VMADDR_CID_LOCAL: Final[int]

if sys.platform != "win32" and sys.platform != "darwin":
    IP_TRANSPARENT: Final[int]
if sys.platform != "win32" and sys.platform != "darwin" and sys.version_info >= (3, 11):
    IP_BIND_ADDRESS_NO_PORT: Final[int]
if sys.version_info >= (3, 12):
    IP_ADD_SOURCE_MEMBERSHIP: Final[int]
    IP_BLOCK_SOURCE: Final[int]
    IP_DROP_SOURCE_MEMBERSHIP: Final[int]
    IP_PKTINFO: Final[int]
    IP_UNBLOCK_SOURCE: Final[int]

IPV6_CHECKSUM: Final[int]
IPV6_JOIN_GROUP: Final[int]
IPV6_LEAVE_GROUP: Final[int]
IPV6_MULTICAST_HOPS: Final[int]
IPV6_MULTICAST_IF: Final[int]
IPV6_MULTICAST_LOOP: Final[int]
IPV6_RECVTCLASS: Final[int]
IPV6_TCLASS: Final[int]
IPV6_UNICAST_HOPS: Final[int]
IPV6_V6ONLY: Final[int]
IPV6_DONTFRAG: Final[int]
IPV6_HOPLIMIT: Final[int]
IPV6_HOPOPTS: Final[int]
IPV6_PKTINFO: Final[int]
IPV6_RECVRTHDR: Final[int]
IPV6_RTHDR: Final[int]
if sys.platform != "win32":
    IPV6_RTHDR_TYPE_0: Final[int]
    IPV6_DSTOPTS: Final[int]
    IPV6_NEXTHOP: Final[int]
    IPV6_PATHMTU: Final[int]
    IPV6_RECVDSTOPTS: Final[int]
    IPV6_RECVHOPLIMIT: Final[int]
    IPV6_RECVHOPOPTS: Final[int]
    IPV6_RECVPATHMTU: Final[int]
    IPV6_RECVPKTINFO: Final[int]
    IPV6_RTHDRDSTOPTS: Final[int]

if sys.platform != "win32" and sys.platform != "linux":
    IPV6_USE_MIN_MTU: Final[int]

EAI_AGAIN: Final[int]
EAI_BADFLAGS: Final[int]
EAI_FAIL: Final[int]
EAI_FAMILY: Final[int]
EAI_MEMORY: Final[int]
EAI_NODATA: Final[int]
EAI_NONAME: Final[int]
EAI_SERVICE: Final[int]
EAI_SOCKTYPE: Final[int]
if sys.platform != "win32":
    EAI_ADDRFAMILY: Final[int]
    EAI_OVERFLOW: Final[int]
    EAI_SYSTEM: Final[int]
if sys.platform != "win32" and sys.platform != "linux":
    EAI_BADHINTS: Final[int]
    EAI_MAX: Final[int]
    EAI_PROTOCOL: Final[int]

AI_ADDRCONFIG: Final[int]
AI_ALL: Final[int]
AI_CANONNAME: Final[int]
AI_NUMERICHOST: Final[int]
AI_NUMERICSERV: Final[int]
AI_PASSIVE: Final[int]
AI_V4MAPPED: Final[int]
if sys.platform != "win32" and sys.platform != "linux":
    AI_DEFAULT: Final[int]
    AI_MASK: Final[int]
    AI_V4MAPPED_CFG: Final[int]

NI_DGRAM: Final[int]
NI_MAXHOST: Final[int]
NI_MAXSERV: Final[int]
NI_NAMEREQD: Final[int]
NI_NOFQDN: Final[int]
NI_NUMERICHOST: Final[int]
NI_NUMERICSERV: Final[int]
if sys.platform == "linux" and sys.version_info >= (3, 13):
    NI_IDN: Final[int]

TCP_FASTOPEN: Final[int]
TCP_KEEPCNT: Final[int]
TCP_KEEPINTVL: Final[int]
TCP_MAXSEG: Final[int]
TCP_NODELAY: Final[int]
if sys.platform != "win32":
    TCP_NOTSENT_LOWAT: Final[int]
if sys.platform != "darwin":
    TCP_KEEPIDLE: Final[int]
if sys.version_info >= (3, 10) and sys.platform == "darwin":
    TCP_KEEPALIVE: Final[int]
if sys.version_info >= (3, 11) and sys.platform == "darwin":
    TCP_CONNECTION_INFO: Final[int]

if sys.platform != "win32" and sys.platform != "darwin":
    TCP_CONGESTION: Final[int]
    TCP_CORK: Final[int]
    TCP_DEFER_ACCEPT: Final[int]
    TCP_INFO: Final[int]
    TCP_LINGER2: Final[int]
    TCP_QUICKACK: Final[int]
    TCP_SYNCNT: Final[int]
    TCP_USER_TIMEOUT: Final[int]
    TCP_WINDOW_CLAMP: Final[int]
if sys.platform == "linux" and sys.version_info >= (3, 12):
    TCP_CC_INFO: Final[int]
    TCP_FASTOPEN_CONNECT: Final[int]
    TCP_FASTOPEN_KEY: Final[int]
    TCP_FASTOPEN_NO_COOKIE: Final[int]
    TCP_INQ: Final[int]
    TCP_MD5SIG: Final[int]
    TCP_MD5SIG_EXT: Final[int]
    TCP_QUEUE_SEQ: Final[int]
    TCP_REPAIR: Final[int]
    TCP_REPAIR_OPTIONS: Final[int]
    TCP_REPAIR_QUEUE: Final[int]
    TCP_REPAIR_WINDOW: Final[int]
    TCP_SAVED_SYN: Final[int]
    TCP_SAVE_SYN: Final[int]
    TCP_THIN_DUPACK: Final[int]
    TCP_THIN_LINEAR_TIMEOUTS: Final[int]
    TCP_TIMESTAMP: Final[int]
    TCP_TX_DELAY: Final[int]
    TCP_ULP: Final[int]
    TCP_ZEROCOPY_RECEIVE: Final[int]

# --------------------
# Specifically documented constants
# --------------------

if sys.platform == "linux":
    # Availability: Linux >= 2.6.25, NetBSD >= 8
    AF_CAN: Final[int]
    PF_CAN: Final[int]
    SOL_CAN_BASE: Final[int]
    SOL_CAN_RAW: Final[int]
    CAN_EFF_FLAG: Final[int]
    CAN_EFF_MASK: Final[int]
    CAN_ERR_FLAG: Final[int]
    CAN_ERR_MASK: Final[int]
    CAN_RAW: Final[int]
    CAN_RAW_FILTER: Final[int]
    CAN_RAW_LOOPBACK: Final[int]
    CAN_RAW_RECV_OWN_MSGS: Final[int]
    CAN_RTR_FLAG: Final[int]
    CAN_SFF_MASK: Final[int]
    if sys.version_info < (3, 11):
        CAN_RAW_ERR_FILTER: Final[int]

if sys.platform == "linux":
    # Availability: Linux >= 2.6.25
    CAN_BCM: Final[int]
    CAN_BCM_TX_SETUP: Final[int]
    CAN_BCM_TX_DELETE: Final[int]
    CAN_BCM_TX_READ: Final[int]
    CAN_BCM_TX_SEND: Final[int]
    CAN_BCM_RX_SETUP: Final[int]
    CAN_BCM_RX_DELETE: Final[int]
    CAN_BCM_RX_READ: Final[int]
    CAN_BCM_TX_STATUS: Final[int]
    CAN_BCM_TX_EXPIRED: Final[int]
    CAN_BCM_RX_STATUS: Final[int]
    CAN_BCM_RX_TIMEOUT: Final[int]
    CAN_BCM_RX_CHANGED: Final[int]
    CAN_BCM_SETTIMER: Final[int]
    CAN_BCM_STARTTIMER: Final[int]
    CAN_BCM_TX_COUNTEVT: Final[int]
    CAN_BCM_TX_ANNOUNCE: Final[int]
    CAN_BCM_TX_CP_CAN_ID: Final[int]
    CAN_BCM_RX_FILTER_ID: Final[int]
    CAN_BCM_RX_CHECK_DLC: Final[int]
    CAN_BCM_RX_NO_AUTOTIMER: Final[int]
    CAN_BCM_RX_ANNOUNCE_RESUME: Final[int]
    CAN_BCM_TX_RESET_MULTI_IDX: Final[int]
    CAN_BCM_RX_RTR_FRAME: Final[int]
    CAN_BCM_CAN_FD_FRAME: Final[int]

if sys.platform == "linux":
    # Availability: Linux >= 3.6
    CAN_RAW_FD_FRAMES: Final[int]
    # Availability: Linux >= 4.1
    CAN_RAW_JOIN_FILTERS: Final[int]
    # Availability: Linux >= 2.6.25
    CAN_ISOTP: Final[int]
    # Availability: Linux >= 5.4
    CAN_J1939: Final[int]

    J1939_MAX_UNICAST_ADDR: Final[int]
    J1939_IDLE_ADDR: Final[int]
    J1939_NO_ADDR: Final[int]
    J1939_NO_NAME: Final[int]
    J1939_PGN_REQUEST: Final[int]
    J1939_PGN_ADDRESS_CLAIMED: Final[int]
    J1939_PGN_ADDRESS_COMMANDED: Final[int]
    J1939_PGN_PDU1_MAX: Final[int]
    J1939_PGN_MAX: Final[int]
    J1939_NO_PGN: Final[int]

    SO_J1939_FILTER: Final[int]
    SO_J1939_PROMISC: Final[int]
    SO_J1939_SEND_PRIO: Final[int]
    SO_J1939_ERRQUEUE: Final[int]

    SCM_J1939_DEST_ADDR: Final[int]
    SCM_J1939_DEST_NAME: Final[int]
    SCM_J1939_PRIO: Final[int]
    SCM_J1939_ERRQUEUE: Final[int]

    J1939_NLA_PAD: Final[int]
    J1939_NLA_BYTES_ACKED: Final[int]
    J1939_EE_INFO_NONE: Final[int]
    J1939_EE_INFO_TX_ABORT: Final[int]
    J1939_FILTER_MAX: Final[int]

if sys.version_info >= (3, 12) and sys.platform != "linux" and sys.platform != "win32" and sys.platform != "darwin":
    # Availability: FreeBSD >= 14.0
    AF_DIVERT: Final[int]
    PF_DIVERT: Final[int]

if sys.platform == "linux":
    # Availability: Linux >= 2.2
    AF_PACKET: Final[int]
    PF_PACKET: Final[int]
    PACKET_BROADCAST: Final[int]
    PACKET_FASTROUTE: Final[int]
    PACKET_HOST: Final[int]
    PACKET_LOOPBACK: Final[int]
    PACKET_MULTICAST: Final[int]
    PACKET_OTHERHOST: Final[int]
    PACKET_OUTGOING: Final[int]

if sys.version_info >= (3, 12) and sys.platform == "linux":
    ETH_P_ALL: Final[int]

if sys.platform == "linux":
    # Availability: Linux >= 2.6.30
    AF_RDS: Final[int]
    PF_RDS: Final[int]
    SOL_RDS: Final[int]
    # These are present in include/linux/rds.h but don't always show up
    # here.
    RDS_CANCEL_SENT_TO: Final[int]
    RDS_CMSG_RDMA_ARGS: Final[int]
    RDS_CMSG_RDMA_DEST: Final[int]
    RDS_CMSG_RDMA_MAP: Final[int]
    RDS_CMSG_RDMA_STATUS: Final[int]
    RDS_CONG_MONITOR: Final[int]
    RDS_FREE_MR: Final[int]
    RDS_GET_MR: Final[int]
    RDS_GET_MR_FOR_DEST: Final[int]
    RDS_RDMA_DONTWAIT: Final[int]
    RDS_RDMA_FENCE: Final[int]
    RDS_RDMA_INVALIDATE: Final[int]
    RDS_RDMA_NOTIFY_ME: Final[int]
    RDS_RDMA_READWRITE: Final[int]
    RDS_RDMA_SILENT: Final[int]
    RDS_RDMA_USE_ONCE: Final[int]
    RDS_RECVERR: Final[int]

    # This is supported by CPython but doesn't seem to be a real thing.
    # The closest existing constant in rds.h is RDS_CMSG_CONG_UPDATE
    # RDS_CMSG_RDMA_UPDATE: Final[int]

if sys.platform == "win32":
    SIO_RCVALL: Final[int]
    SIO_KEEPALIVE_VALS: Final[int]
    SIO_LOOPBACK_FAST_PATH: Final[int]
    RCVALL_MAX: Final[int]
    RCVALL_OFF: Final[int]
    RCVALL_ON: Final[int]
    RCVALL_SOCKETLEVELONLY: Final[int]

if sys.platform == "linux":
    AF_TIPC: Final[int]
    SOL_TIPC: Final[int]
    TIPC_ADDR_ID: Final[int]
    TIPC_ADDR_NAME: Final[int]
    TIPC_ADDR_NAMESEQ: Final[int]
    TIPC_CFG_SRV: Final[int]
    TIPC_CLUSTER_SCOPE: Final[int]
    TIPC_CONN_TIMEOUT: Final[int]
    TIPC_CRITICAL_IMPORTANCE: Final[int]
    TIPC_DEST_DROPPABLE: Final[int]
    TIPC_HIGH_IMPORTANCE: Final[int]
    TIPC_IMPORTANCE: Final[int]
    TIPC_LOW_IMPORTANCE: Final[int]
    TIPC_MEDIUM_IMPORTANCE: Final[int]
    TIPC_NODE_SCOPE: Final[int]
    TIPC_PUBLISHED: Final[int]
    TIPC_SRC_DROPPABLE: Final[int]
    TIPC_SUBSCR_TIMEOUT: Final[int]
    TIPC_SUB_CANCEL: Final[int]
    TIPC_SUB_PORTS: Final[int]
    TIPC_SUB_SERVICE: Final[int]
    TIPC_TOP_SRV: Final[int]
    TIPC_WAIT_FOREVER: Final[int]
    TIPC_WITHDRAWN: Final[int]
    TIPC_ZONE_SCOPE: Final[int]

if sys.platform == "linux":
    # Availability: Linux >= 2.6.38
    AF_ALG: Final[int]
    SOL_ALG: Final[int]
    ALG_OP_DECRYPT: Final[int]
    ALG_OP_ENCRYPT: Final[int]
    ALG_OP_SIGN: Final[int]
    ALG_OP_VERIFY: Final[int]
    ALG_SET_AEAD_ASSOCLEN: Final[int]
    ALG_SET_AEAD_AUTHSIZE: Final[int]
    ALG_SET_IV: Final[int]
    ALG_SET_KEY: Final[int]
    ALG_SET_OP: Final[int]
    ALG_SET_PUBKEY: Final[int]

if sys.platform == "linux":
    # Availability: Linux >= 4.8 (or maybe 3.9, CPython docs are confusing)
    AF_VSOCK: Final[int]
    IOCTL_VM_SOCKETS_GET_LOCAL_CID: Final = 0x7B9
    VMADDR_CID_ANY: Final = 0xFFFFFFFF
    VMADDR_CID_HOST: Final = 2
    VMADDR_PORT_ANY: Final = 0xFFFFFFFF
    SO_VM_SOCKETS_BUFFER_MAX_SIZE: Final = 2
    SO_VM_SOCKETS_BUFFER_SIZE: Final = 0
    SO_VM_SOCKETS_BUFFER_MIN_SIZE: Final = 1
    VM_SOCKETS_INVALID_VERSION: Final = 0xFFFFFFFF  # undocumented

# Documented as only available on BSD, macOS, but empirically sometimes
# available on Windows
if sys.platform != "linux":
    AF_LINK: Final[int]

has_ipv6: bool

if sys.platform != "darwin" and sys.platform != "linux":
    BDADDR_ANY: Final = "00:00:00:00:00:00"
    BDADDR_LOCAL: Final = "00:00:00:FF:FF:FF"

if sys.platform != "win32" and sys.platform != "darwin" and sys.platform != "linux":
    HCI_FILTER: Final[int]  # not in NetBSD or DragonFlyBSD
    HCI_TIME_STAMP: Final[int]  # not in FreeBSD, NetBSD, or DragonFlyBSD
    HCI_DATA_DIR: Final[int]  # not in FreeBSD, NetBSD, or DragonFlyBSD

if sys.platform == "linux":
    AF_QIPCRTR: Final[int]  # Availability: Linux >= 4.7

if sys.version_info >= (3, 11) and sys.platform != "linux" and sys.platform != "win32" and sys.platform != "darwin":
    # FreeBSD
    SCM_CREDS2: Final[int]
    LOCAL_CREDS: Final[int]
    LOCAL_CREDS_PERSISTENT: Final[int]

if sys.version_info >= (3, 11) and sys.platform == "linux":
    SO_INCOMING_CPU: Final[int]  # Availability: Linux >= 3.9

if sys.version_info >= (3, 12) and sys.platform == "win32":
    # Availability: Windows
    AF_HYPERV: Final[int]
    HV_PROTOCOL_RAW: Final[int]
    HVSOCKET_CONNECT_TIMEOUT: Final[int]
    HVSOCKET_CONNECT_TIMEOUT_MAX: Final[int]
    HVSOCKET_CONNECTED_SUSPEND: Final[int]
    HVSOCKET_ADDRESS_FLAG_PASSTHRU: Final[int]
    HV_GUID_ZERO: Final = "00000000-0000-0000-0000-000000000000"
    HV_GUID_WILDCARD: Final = "00000000-0000-0000-0000-000000000000"
    HV_GUID_BROADCAST: Final = "FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF"
    HV_GUID_CHILDREN: Final = "90DB8B89-0D35-4F79-8CE9-49EA0AC8B7CD"
    HV_GUID_LOOPBACK: Final = "E0E16197-DD56-4A10-9195-5EE7A155A838"
    HV_GUID_PARENT: Final = "A42E7CDA-D03F-480C-9CC2-A4DE20ABB878"

if sys.version_info >= (3, 12):
    if sys.platform != "win32":
        # Availability: Linux, FreeBSD, macOS
        ETHERTYPE_ARP: Final[int]
        ETHERTYPE_IP: Final[int]
        ETHERTYPE_IPV6: Final[int]
        ETHERTYPE_VLAN: Final[int]

# --------------------
# Semi-documented constants
# These are alluded to under the "Socket families" section in the docs
# https://docs.python.org/3/library/socket.html#socket-families
# --------------------

if sys.platform == "linux":
    # Netlink is defined by Linux
    AF_NETLINK: Final[int]
    NETLINK_CRYPTO: Final[int]
    NETLINK_DNRTMSG: Final[int]
    NETLINK_FIREWALL: Final[int]
    NETLINK_IP6_FW: Final[int]
    NETLINK_NFLOG: Final[int]
    NETLINK_ROUTE: Final[int]
    NETLINK_USERSOCK: Final[int]
    NETLINK_XFRM: Final[int]
    # Technically still supported by CPython
    # NETLINK_ARPD: Final[int]  # linux 2.0 to 2.6.12 (EOL August 2005)
    # NETLINK_ROUTE6: Final[int]  # linux 2.2 to 2.6.12 (EOL August 2005)
    # NETLINK_SKIP: Final[int]  # linux 2.0 to 2.6.12 (EOL August 2005)
    # NETLINK_TAPBASE: Final[int]  # linux 2.2 to 2.6.12 (EOL August 2005)
    # NETLINK_TCPDIAG: Final[int]  # linux 2.6.0 to 2.6.13 (EOL December 2005)
    # NETLINK_W1: Final[int]  # linux 2.6.13 to 2.6.17 (EOL October 2006)

if sys.platform == "darwin":
    PF_SYSTEM: Final[int]
    SYSPROTO_CONTROL: Final[int]

if sys.platform != "darwin" and sys.platform != "linux":
    AF_BLUETOOTH: Final[int]

if sys.platform != "win32" and sys.platform != "darwin" and sys.platform != "linux":
    # Linux and some BSD support is explicit in the docs
    # Windows and macOS do not support in practice
    BTPROTO_HCI: Final[int]
    BTPROTO_L2CAP: Final[int]
    BTPROTO_SCO: Final[int]  # not in FreeBSD
if sys.platform != "darwin" and sys.platform != "linux":
    BTPROTO_RFCOMM: Final[int]

if sys.platform == "linux":
    UDPLITE_RECV_CSCOV: Final[int]
    UDPLITE_SEND_CSCOV: Final[int]

# --------------------
# Documented under socket.shutdown
# --------------------
SHUT_RD: Final[int]
SHUT_RDWR: Final[int]
SHUT_WR: Final[int]

# --------------------
# Undocumented constants
# --------------------

# Undocumented address families
AF_APPLETALK: Final[int]
AF_DECnet: Final[int]
AF_IPX: Final[int]
AF_SNA: Final[int]

if sys.platform != "win32":
    AF_ROUTE: Final[int]

if sys.platform == "darwin":
    AF_SYSTEM: Final[int]

if sys.platform != "darwin":
    AF_IRDA: Final[int]

if sys.platform != "win32" and sys.platform != "darwin":
    AF_ASH: Final[int]
    AF_ATMPVC: Final[int]
    AF_ATMSVC: Final[int]
    AF_AX25: Final[int]
    AF_BRIDGE: Final[int]
    AF_ECONET: Final[int]
    AF_KEY: Final[int]
    AF_LLC: Final[int]
    AF_NETBEUI: Final[int]
    AF_NETROM: Final[int]
    AF_PPPOX: Final[int]
    AF_ROSE: Final[int]
    AF_SECURITY: Final[int]
    AF_WANPIPE: Final[int]
    AF_X25: Final[int]

# Miscellaneous undocumented

if sys.platform != "win32" and sys.platform != "linux":
    LOCAL_PEERCRED: Final[int]

if sys.platform != "win32" and sys.platform != "darwin":
    # Defined in linux socket.h, but this isn't always present for
    # some reason.
    IPX_TYPE: Final[int]

# ===== Classes =====

@disjoint_base
class socket:
    @property
    def family(self) -> int: ...
    @property
    def type(self) -> int: ...
    @property
    def proto(self) -> int: ...
    # F811: "Redefinition of unused `timeout`"
    @property
    def timeout(self) -> float | None: ...  # noqa: F811
    if sys.platform == "win32":
        def __init__(
            self, family: int = ..., type: int = ..., proto: int = ..., fileno: SupportsIndex | bytes | None = None
        ) -> None: ...
    else:
        def __init__(self, family: int = ..., type: int = ..., proto: int = ..., fileno: SupportsIndex | None = None) -> None: ...

    def bind(self, address: _Address, /) -> None: ...
    def close(self) -> None: ...
    def connect(self, address: _Address, /) -> None: ...
    def connect_ex(self, address: _Address, /) -> int: ...
    def detach(self) -> int: ...
    def fileno(self) -> int: ...
    def getpeername(self) -> _RetAddress: ...
    def getsockname(self) -> _RetAddress: ...
    @overload
    def getsockopt(self, level: int, optname: int, /) -> int: ...
    @overload
    def getsockopt(self, level: int, optname: int, buflen: int, /) -> bytes: ...
    def getblocking(self) -> bool: ...
    def gettimeout(self) -> float | None: ...
    if sys.platform == "win32":
        def ioctl(self, control: int, option: int | tuple[int, int, int] | bool, /) -> None: ...

    def listen(self, backlog: int = ..., /) -> None: ...
    def recv(self, bufsize: int, flags: int = 0, /) -> bytes: ...
    def recvfrom(self, bufsize: int, flags: int = 0, /) -> tuple[bytes, _RetAddress]: ...
    if sys.platform != "win32":
        def recvmsg(self, bufsize: int, ancbufsize: int = 0, flags: int = 0, /) -> tuple[bytes, list[_CMSG], int, Any]: ...
        def recvmsg_into(
            self, buffers: Iterable[WriteableBuffer], ancbufsize: int = 0, flags: int = 0, /
        ) -> tuple[int, list[_CMSG], int, Any]: ...

    def recvfrom_into(self, buffer: WriteableBuffer, nbytes: int = 0, flags: int = 0) -> tuple[int, _RetAddress]: ...
    def recv_into(self, buffer: WriteableBuffer, nbytes: int = 0, flags: int = 0) -> int: ...
    def send(self, data: ReadableBuffer, flags: int = 0, /) -> int: ...
    def sendall(self, data: ReadableBuffer, flags: int = 0, /) -> None: ...
    @overload
    def sendto(self, data: ReadableBuffer, address: _Address, /) -> int: ...
    @overload
    def sendto(self, data: ReadableBuffer, flags: int, address: _Address, /) -> int: ...
    if sys.platform != "win32":
        def sendmsg(
            self,
            buffers: Iterable[ReadableBuffer],
            ancdata: Iterable[_CMSGArg] = ...,
            flags: int = 0,
            address: _Address | None = None,
            /,
        ) -> int: ...
    if sys.platform == "linux":
        def sendmsg_afalg(
            self, msg: Iterable[ReadableBuffer] = ..., *, op: int, iv: Any = ..., assoclen: int = ..., flags: int = 0
        ) -> int: ...

    def setblocking(self, flag: bool, /) -> None: ...
    def settimeout(self, value: float | None, /) -> None: ...
    @overload
    def setsockopt(self, level: int, optname: int, value: int | ReadableBuffer, /) -> None: ...
    @overload
    def setsockopt(self, level: int, optname: int, value: None, optlen: int, /) -> None: ...
    if sys.platform == "win32":
        def share(self, process_id: int, /) -> bytes: ...

    def shutdown(self, how: int, /) -> None: ...

SocketType = socket

# ===== Functions =====

def close(fd: SupportsIndex, /) -> None: ...
def dup(fd: SupportsIndex, /) -> int: ...

# the 5th tuple item is an address
def getaddrinfo(
    host: bytes | str | None, port: bytes | str | int | None, family: int = ..., type: int = 0, proto: int = 0, flags: int = 0
) -> list[tuple[int, int, int, str, tuple[str, int] | tuple[str, int, int, int] | tuple[int, bytes]]]: ...
def gethostbyname(hostname: str, /) -> str: ...
def gethostbyname_ex(hostname: str, /) -> tuple[str, list[str], list[str]]: ...
def gethostname() -> str: ...
def gethostbyaddr(ip_address: str, /) -> tuple[str, list[str], list[str]]: ...
def getnameinfo(sockaddr: tuple[str, int] | tuple[str, int, int, int] | tuple[int, bytes], flags: int, /) -> tuple[str, str]: ...
def getprotobyname(protocolname: str, /) -> int: ...
def getservbyname(servicename: str, protocolname: str = ..., /) -> int: ...
def getservbyport(port: int, protocolname: str = ..., /) -> str: ...
def ntohl(x: int, /) -> int: ...  # param & ret val are 32-bit ints
def ntohs(x: int, /) -> int: ...  # param & ret val are 16-bit ints
def htonl(x: int, /) -> int: ...  # param & ret val are 32-bit ints
def htons(x: int, /) -> int: ...  # param & ret val are 16-bit ints
def inet_aton(ip_addr: str, /) -> bytes: ...  # ret val 4 bytes in length
def inet_ntoa(packed_ip: ReadableBuffer, /) -> str: ...
def inet_pton(address_family: int, ip_string: str, /) -> bytes: ...
def inet_ntop(address_family: int, packed_ip: ReadableBuffer, /) -> str: ...
def getdefaulttimeout() -> float | None: ...

# F811: "Redefinition of unused `timeout`"
def setdefaulttimeout(timeout: float | None, /) -> None: ...  # noqa: F811

if sys.platform != "win32":
    def sethostname(name: str, /) -> None: ...
    def CMSG_LEN(length: int, /) -> int: ...
    def CMSG_SPACE(length: int, /) -> int: ...
    def socketpair(family: int = ..., type: int = ..., proto: int = 0, /) -> tuple[socket, socket]: ...

def if_nameindex() -> list[tuple[int, str]]: ...
def if_nametoindex(oname: str, /) -> int: ...

if sys.version_info >= (3, 14):
    def if_indextoname(if_index: int, /) -> str: ...

else:
    def if_indextoname(index: int, /) -> str: ...

CAPI: CapsuleType
