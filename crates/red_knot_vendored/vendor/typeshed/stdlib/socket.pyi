# Ideally, we'd just do "from _socket import *". Unfortunately, socket
# overrides some definitions from _socket incompatibly. mypy incorrectly
# prefers the definitions from _socket over those defined here.
import _socket
import sys
from _socket import (
    CAPI as CAPI,
    EAI_AGAIN as EAI_AGAIN,
    EAI_BADFLAGS as EAI_BADFLAGS,
    EAI_FAIL as EAI_FAIL,
    EAI_FAMILY as EAI_FAMILY,
    EAI_MEMORY as EAI_MEMORY,
    EAI_NODATA as EAI_NODATA,
    EAI_NONAME as EAI_NONAME,
    EAI_SERVICE as EAI_SERVICE,
    EAI_SOCKTYPE as EAI_SOCKTYPE,
    INADDR_ALLHOSTS_GROUP as INADDR_ALLHOSTS_GROUP,
    INADDR_ANY as INADDR_ANY,
    INADDR_BROADCAST as INADDR_BROADCAST,
    INADDR_LOOPBACK as INADDR_LOOPBACK,
    INADDR_MAX_LOCAL_GROUP as INADDR_MAX_LOCAL_GROUP,
    INADDR_NONE as INADDR_NONE,
    INADDR_UNSPEC_GROUP as INADDR_UNSPEC_GROUP,
    IP_ADD_MEMBERSHIP as IP_ADD_MEMBERSHIP,
    IP_DROP_MEMBERSHIP as IP_DROP_MEMBERSHIP,
    IP_HDRINCL as IP_HDRINCL,
    IP_MULTICAST_IF as IP_MULTICAST_IF,
    IP_MULTICAST_LOOP as IP_MULTICAST_LOOP,
    IP_MULTICAST_TTL as IP_MULTICAST_TTL,
    IP_OPTIONS as IP_OPTIONS,
    IP_RECVDSTADDR as IP_RECVDSTADDR,
    IP_TOS as IP_TOS,
    IP_TTL as IP_TTL,
    IPPORT_RESERVED as IPPORT_RESERVED,
    IPPORT_USERRESERVED as IPPORT_USERRESERVED,
    IPPROTO_AH as IPPROTO_AH,
    IPPROTO_DSTOPTS as IPPROTO_DSTOPTS,
    IPPROTO_EGP as IPPROTO_EGP,
    IPPROTO_ESP as IPPROTO_ESP,
    IPPROTO_FRAGMENT as IPPROTO_FRAGMENT,
    IPPROTO_GGP as IPPROTO_GGP,
    IPPROTO_HOPOPTS as IPPROTO_HOPOPTS,
    IPPROTO_ICMP as IPPROTO_ICMP,
    IPPROTO_ICMPV6 as IPPROTO_ICMPV6,
    IPPROTO_IDP as IPPROTO_IDP,
    IPPROTO_IGMP as IPPROTO_IGMP,
    IPPROTO_IP as IPPROTO_IP,
    IPPROTO_IPV4 as IPPROTO_IPV4,
    IPPROTO_IPV6 as IPPROTO_IPV6,
    IPPROTO_MAX as IPPROTO_MAX,
    IPPROTO_ND as IPPROTO_ND,
    IPPROTO_NONE as IPPROTO_NONE,
    IPPROTO_PIM as IPPROTO_PIM,
    IPPROTO_PUP as IPPROTO_PUP,
    IPPROTO_RAW as IPPROTO_RAW,
    IPPROTO_ROUTING as IPPROTO_ROUTING,
    IPPROTO_SCTP as IPPROTO_SCTP,
    IPPROTO_TCP as IPPROTO_TCP,
    IPPROTO_UDP as IPPROTO_UDP,
    IPV6_CHECKSUM as IPV6_CHECKSUM,
    IPV6_JOIN_GROUP as IPV6_JOIN_GROUP,
    IPV6_LEAVE_GROUP as IPV6_LEAVE_GROUP,
    IPV6_MULTICAST_HOPS as IPV6_MULTICAST_HOPS,
    IPV6_MULTICAST_IF as IPV6_MULTICAST_IF,
    IPV6_MULTICAST_LOOP as IPV6_MULTICAST_LOOP,
    IPV6_RECVTCLASS as IPV6_RECVTCLASS,
    IPV6_TCLASS as IPV6_TCLASS,
    IPV6_UNICAST_HOPS as IPV6_UNICAST_HOPS,
    IPV6_V6ONLY as IPV6_V6ONLY,
    NI_DGRAM as NI_DGRAM,
    NI_MAXHOST as NI_MAXHOST,
    NI_MAXSERV as NI_MAXSERV,
    NI_NAMEREQD as NI_NAMEREQD,
    NI_NOFQDN as NI_NOFQDN,
    NI_NUMERICHOST as NI_NUMERICHOST,
    NI_NUMERICSERV as NI_NUMERICSERV,
    SHUT_RD as SHUT_RD,
    SHUT_RDWR as SHUT_RDWR,
    SHUT_WR as SHUT_WR,
    SO_ACCEPTCONN as SO_ACCEPTCONN,
    SO_BROADCAST as SO_BROADCAST,
    SO_DEBUG as SO_DEBUG,
    SO_DONTROUTE as SO_DONTROUTE,
    SO_ERROR as SO_ERROR,
    SO_KEEPALIVE as SO_KEEPALIVE,
    SO_LINGER as SO_LINGER,
    SO_OOBINLINE as SO_OOBINLINE,
    SO_RCVBUF as SO_RCVBUF,
    SO_RCVLOWAT as SO_RCVLOWAT,
    SO_RCVTIMEO as SO_RCVTIMEO,
    SO_REUSEADDR as SO_REUSEADDR,
    SO_SNDBUF as SO_SNDBUF,
    SO_SNDLOWAT as SO_SNDLOWAT,
    SO_SNDTIMEO as SO_SNDTIMEO,
    SO_TYPE as SO_TYPE,
    SO_USELOOPBACK as SO_USELOOPBACK,
    SOL_IP as SOL_IP,
    SOL_SOCKET as SOL_SOCKET,
    SOL_TCP as SOL_TCP,
    SOL_UDP as SOL_UDP,
    SOMAXCONN as SOMAXCONN,
    TCP_FASTOPEN as TCP_FASTOPEN,
    TCP_KEEPCNT as TCP_KEEPCNT,
    TCP_KEEPINTVL as TCP_KEEPINTVL,
    TCP_MAXSEG as TCP_MAXSEG,
    TCP_NODELAY as TCP_NODELAY,
    SocketType as SocketType,
    _Address as _Address,
    _RetAddress as _RetAddress,
    close as close,
    dup as dup,
    getdefaulttimeout as getdefaulttimeout,
    gethostbyaddr as gethostbyaddr,
    gethostbyname as gethostbyname,
    gethostbyname_ex as gethostbyname_ex,
    gethostname as gethostname,
    getnameinfo as getnameinfo,
    getprotobyname as getprotobyname,
    getservbyname as getservbyname,
    getservbyport as getservbyport,
    has_ipv6 as has_ipv6,
    htonl as htonl,
    htons as htons,
    if_indextoname as if_indextoname,
    if_nameindex as if_nameindex,
    if_nametoindex as if_nametoindex,
    inet_aton as inet_aton,
    inet_ntoa as inet_ntoa,
    inet_ntop as inet_ntop,
    inet_pton as inet_pton,
    ntohl as ntohl,
    ntohs as ntohs,
    setdefaulttimeout as setdefaulttimeout,
)
from _typeshed import ReadableBuffer, Unused, WriteableBuffer
from collections.abc import Iterable
from enum import IntEnum, IntFlag
from io import BufferedReader, BufferedRWPair, BufferedWriter, IOBase, RawIOBase, TextIOWrapper
from typing import Any, Literal, Protocol, SupportsIndex, overload
from typing_extensions import Self

if sys.platform == "win32":
    from _socket import (
        RCVALL_MAX as RCVALL_MAX,
        RCVALL_OFF as RCVALL_OFF,
        RCVALL_ON as RCVALL_ON,
        RCVALL_SOCKETLEVELONLY as RCVALL_SOCKETLEVELONLY,
        SIO_KEEPALIVE_VALS as SIO_KEEPALIVE_VALS,
        SIO_LOOPBACK_FAST_PATH as SIO_LOOPBACK_FAST_PATH,
        SIO_RCVALL as SIO_RCVALL,
        SO_EXCLUSIVEADDRUSE as SO_EXCLUSIVEADDRUSE,
    )

if sys.platform != "darwin" or sys.version_info >= (3, 9):
    from _socket import (
        IPV6_DONTFRAG as IPV6_DONTFRAG,
        IPV6_HOPLIMIT as IPV6_HOPLIMIT,
        IPV6_HOPOPTS as IPV6_HOPOPTS,
        IPV6_PKTINFO as IPV6_PKTINFO,
        IPV6_RECVRTHDR as IPV6_RECVRTHDR,
        IPV6_RTHDR as IPV6_RTHDR,
    )

if sys.platform == "darwin":
    from _socket import PF_SYSTEM as PF_SYSTEM, SYSPROTO_CONTROL as SYSPROTO_CONTROL

if sys.platform != "darwin":
    from _socket import (
        IPPROTO_CBT as IPPROTO_CBT,
        IPPROTO_ICLFXBM as IPPROTO_ICLFXBM,
        IPPROTO_IGP as IPPROTO_IGP,
        IPPROTO_L2TP as IPPROTO_L2TP,
        IPPROTO_PGM as IPPROTO_PGM,
        IPPROTO_RDP as IPPROTO_RDP,
        IPPROTO_ST as IPPROTO_ST,
        TCP_KEEPIDLE as TCP_KEEPIDLE,
    )

if sys.version_info >= (3, 10):
    from _socket import IP_RECVTOS as IP_RECVTOS
elif sys.platform != "win32" and sys.platform != "darwin":
    from _socket import IP_RECVTOS as IP_RECVTOS

if sys.platform != "win32" and sys.platform != "darwin":
    from _socket import (
        IP_BIND_ADDRESS_NO_PORT as IP_BIND_ADDRESS_NO_PORT,
        IP_TRANSPARENT as IP_TRANSPARENT,
        IPPROTO_BIP as IPPROTO_BIP,
        IPPROTO_MOBILE as IPPROTO_MOBILE,
        IPPROTO_VRRP as IPPROTO_VRRP,
        IPX_TYPE as IPX_TYPE,
        SCM_CREDENTIALS as SCM_CREDENTIALS,
        SO_BINDTODEVICE as SO_BINDTODEVICE,
        SO_DOMAIN as SO_DOMAIN,
        SO_MARK as SO_MARK,
        SO_PASSCRED as SO_PASSCRED,
        SO_PASSSEC as SO_PASSSEC,
        SO_PEERCRED as SO_PEERCRED,
        SO_PEERSEC as SO_PEERSEC,
        SO_PRIORITY as SO_PRIORITY,
        SO_PROTOCOL as SO_PROTOCOL,
        SO_SETFIB as SO_SETFIB,
        SOL_ATALK as SOL_ATALK,
        SOL_AX25 as SOL_AX25,
        SOL_HCI as SOL_HCI,
        SOL_IPX as SOL_IPX,
        SOL_NETROM as SOL_NETROM,
        SOL_ROSE as SOL_ROSE,
        TCP_CONGESTION as TCP_CONGESTION,
        TCP_CORK as TCP_CORK,
        TCP_DEFER_ACCEPT as TCP_DEFER_ACCEPT,
        TCP_INFO as TCP_INFO,
        TCP_LINGER2 as TCP_LINGER2,
        TCP_QUICKACK as TCP_QUICKACK,
        TCP_SYNCNT as TCP_SYNCNT,
        TCP_USER_TIMEOUT as TCP_USER_TIMEOUT,
        TCP_WINDOW_CLAMP as TCP_WINDOW_CLAMP,
    )

if sys.platform != "win32":
    from _socket import (
        CMSG_LEN as CMSG_LEN,
        CMSG_SPACE as CMSG_SPACE,
        EAI_ADDRFAMILY as EAI_ADDRFAMILY,
        EAI_BADHINTS as EAI_BADHINTS,
        EAI_MAX as EAI_MAX,
        EAI_OVERFLOW as EAI_OVERFLOW,
        EAI_PROTOCOL as EAI_PROTOCOL,
        EAI_SYSTEM as EAI_SYSTEM,
        IP_DEFAULT_MULTICAST_LOOP as IP_DEFAULT_MULTICAST_LOOP,
        IP_DEFAULT_MULTICAST_TTL as IP_DEFAULT_MULTICAST_TTL,
        IP_MAX_MEMBERSHIPS as IP_MAX_MEMBERSHIPS,
        IP_RECVOPTS as IP_RECVOPTS,
        IP_RECVRETOPTS as IP_RECVRETOPTS,
        IP_RETOPTS as IP_RETOPTS,
        IPPROTO_EON as IPPROTO_EON,
        IPPROTO_GRE as IPPROTO_GRE,
        IPPROTO_HELLO as IPPROTO_HELLO,
        IPPROTO_IPCOMP as IPPROTO_IPCOMP,
        IPPROTO_IPIP as IPPROTO_IPIP,
        IPPROTO_RSVP as IPPROTO_RSVP,
        IPPROTO_TP as IPPROTO_TP,
        IPPROTO_XTP as IPPROTO_XTP,
        IPV6_RTHDR_TYPE_0 as IPV6_RTHDR_TYPE_0,
        LOCAL_PEERCRED as LOCAL_PEERCRED,
        SCM_CREDS as SCM_CREDS,
        SCM_RIGHTS as SCM_RIGHTS,
        SO_REUSEPORT as SO_REUSEPORT,
        TCP_NOTSENT_LOWAT as TCP_NOTSENT_LOWAT,
        sethostname as sethostname,
    )

    if sys.platform != "darwin" or sys.version_info >= (3, 9):
        from _socket import (
            IPV6_DSTOPTS as IPV6_DSTOPTS,
            IPV6_NEXTHOP as IPV6_NEXTHOP,
            IPV6_PATHMTU as IPV6_PATHMTU,
            IPV6_RECVDSTOPTS as IPV6_RECVDSTOPTS,
            IPV6_RECVHOPLIMIT as IPV6_RECVHOPLIMIT,
            IPV6_RECVHOPOPTS as IPV6_RECVHOPOPTS,
            IPV6_RECVPATHMTU as IPV6_RECVPATHMTU,
            IPV6_RECVPKTINFO as IPV6_RECVPKTINFO,
            IPV6_RTHDRDSTOPTS as IPV6_RTHDRDSTOPTS,
            IPV6_USE_MIN_MTU as IPV6_USE_MIN_MTU,
        )

if sys.platform != "darwin":
    if sys.platform != "win32" or sys.version_info >= (3, 9):
        from _socket import BDADDR_ANY as BDADDR_ANY, BDADDR_LOCAL as BDADDR_LOCAL, BTPROTO_RFCOMM as BTPROTO_RFCOMM

if sys.platform == "darwin" and sys.version_info >= (3, 10):
    from _socket import TCP_KEEPALIVE as TCP_KEEPALIVE

if sys.platform == "darwin" and sys.version_info >= (3, 11):
    from _socket import TCP_CONNECTION_INFO as TCP_CONNECTION_INFO

if sys.platform == "linux":
    from _socket import (
        ALG_OP_DECRYPT as ALG_OP_DECRYPT,
        ALG_OP_ENCRYPT as ALG_OP_ENCRYPT,
        ALG_OP_SIGN as ALG_OP_SIGN,
        ALG_OP_VERIFY as ALG_OP_VERIFY,
        ALG_SET_AEAD_ASSOCLEN as ALG_SET_AEAD_ASSOCLEN,
        ALG_SET_AEAD_AUTHSIZE as ALG_SET_AEAD_AUTHSIZE,
        ALG_SET_IV as ALG_SET_IV,
        ALG_SET_KEY as ALG_SET_KEY,
        ALG_SET_OP as ALG_SET_OP,
        ALG_SET_PUBKEY as ALG_SET_PUBKEY,
        CAN_BCM as CAN_BCM,
        CAN_BCM_CAN_FD_FRAME as CAN_BCM_CAN_FD_FRAME,
        CAN_BCM_RX_ANNOUNCE_RESUME as CAN_BCM_RX_ANNOUNCE_RESUME,
        CAN_BCM_RX_CHANGED as CAN_BCM_RX_CHANGED,
        CAN_BCM_RX_CHECK_DLC as CAN_BCM_RX_CHECK_DLC,
        CAN_BCM_RX_DELETE as CAN_BCM_RX_DELETE,
        CAN_BCM_RX_FILTER_ID as CAN_BCM_RX_FILTER_ID,
        CAN_BCM_RX_NO_AUTOTIMER as CAN_BCM_RX_NO_AUTOTIMER,
        CAN_BCM_RX_READ as CAN_BCM_RX_READ,
        CAN_BCM_RX_RTR_FRAME as CAN_BCM_RX_RTR_FRAME,
        CAN_BCM_RX_SETUP as CAN_BCM_RX_SETUP,
        CAN_BCM_RX_STATUS as CAN_BCM_RX_STATUS,
        CAN_BCM_RX_TIMEOUT as CAN_BCM_RX_TIMEOUT,
        CAN_BCM_SETTIMER as CAN_BCM_SETTIMER,
        CAN_BCM_STARTTIMER as CAN_BCM_STARTTIMER,
        CAN_BCM_TX_ANNOUNCE as CAN_BCM_TX_ANNOUNCE,
        CAN_BCM_TX_COUNTEVT as CAN_BCM_TX_COUNTEVT,
        CAN_BCM_TX_CP_CAN_ID as CAN_BCM_TX_CP_CAN_ID,
        CAN_BCM_TX_DELETE as CAN_BCM_TX_DELETE,
        CAN_BCM_TX_EXPIRED as CAN_BCM_TX_EXPIRED,
        CAN_BCM_TX_READ as CAN_BCM_TX_READ,
        CAN_BCM_TX_RESET_MULTI_IDX as CAN_BCM_TX_RESET_MULTI_IDX,
        CAN_BCM_TX_SEND as CAN_BCM_TX_SEND,
        CAN_BCM_TX_SETUP as CAN_BCM_TX_SETUP,
        CAN_BCM_TX_STATUS as CAN_BCM_TX_STATUS,
        CAN_EFF_FLAG as CAN_EFF_FLAG,
        CAN_EFF_MASK as CAN_EFF_MASK,
        CAN_ERR_FLAG as CAN_ERR_FLAG,
        CAN_ERR_MASK as CAN_ERR_MASK,
        CAN_ISOTP as CAN_ISOTP,
        CAN_RAW as CAN_RAW,
        CAN_RAW_ERR_FILTER as CAN_RAW_ERR_FILTER,
        CAN_RAW_FD_FRAMES as CAN_RAW_FD_FRAMES,
        CAN_RAW_FILTER as CAN_RAW_FILTER,
        CAN_RAW_LOOPBACK as CAN_RAW_LOOPBACK,
        CAN_RAW_RECV_OWN_MSGS as CAN_RAW_RECV_OWN_MSGS,
        CAN_RTR_FLAG as CAN_RTR_FLAG,
        CAN_SFF_MASK as CAN_SFF_MASK,
        IOCTL_VM_SOCKETS_GET_LOCAL_CID as IOCTL_VM_SOCKETS_GET_LOCAL_CID,
        NETLINK_ARPD as NETLINK_ARPD,
        NETLINK_CRYPTO as NETLINK_CRYPTO,
        NETLINK_DNRTMSG as NETLINK_DNRTMSG,
        NETLINK_FIREWALL as NETLINK_FIREWALL,
        NETLINK_IP6_FW as NETLINK_IP6_FW,
        NETLINK_NFLOG as NETLINK_NFLOG,
        NETLINK_ROUTE as NETLINK_ROUTE,
        NETLINK_ROUTE6 as NETLINK_ROUTE6,
        NETLINK_SKIP as NETLINK_SKIP,
        NETLINK_TAPBASE as NETLINK_TAPBASE,
        NETLINK_TCPDIAG as NETLINK_TCPDIAG,
        NETLINK_USERSOCK as NETLINK_USERSOCK,
        NETLINK_W1 as NETLINK_W1,
        NETLINK_XFRM as NETLINK_XFRM,
        PACKET_BROADCAST as PACKET_BROADCAST,
        PACKET_FASTROUTE as PACKET_FASTROUTE,
        PACKET_HOST as PACKET_HOST,
        PACKET_LOOPBACK as PACKET_LOOPBACK,
        PACKET_MULTICAST as PACKET_MULTICAST,
        PACKET_OTHERHOST as PACKET_OTHERHOST,
        PACKET_OUTGOING as PACKET_OUTGOING,
        PF_CAN as PF_CAN,
        PF_PACKET as PF_PACKET,
        PF_RDS as PF_RDS,
        RDS_CANCEL_SENT_TO as RDS_CANCEL_SENT_TO,
        RDS_CMSG_RDMA_ARGS as RDS_CMSG_RDMA_ARGS,
        RDS_CMSG_RDMA_DEST as RDS_CMSG_RDMA_DEST,
        RDS_CMSG_RDMA_MAP as RDS_CMSG_RDMA_MAP,
        RDS_CMSG_RDMA_STATUS as RDS_CMSG_RDMA_STATUS,
        RDS_CMSG_RDMA_UPDATE as RDS_CMSG_RDMA_UPDATE,
        RDS_CONG_MONITOR as RDS_CONG_MONITOR,
        RDS_FREE_MR as RDS_FREE_MR,
        RDS_GET_MR as RDS_GET_MR,
        RDS_GET_MR_FOR_DEST as RDS_GET_MR_FOR_DEST,
        RDS_RDMA_DONTWAIT as RDS_RDMA_DONTWAIT,
        RDS_RDMA_FENCE as RDS_RDMA_FENCE,
        RDS_RDMA_INVALIDATE as RDS_RDMA_INVALIDATE,
        RDS_RDMA_NOTIFY_ME as RDS_RDMA_NOTIFY_ME,
        RDS_RDMA_READWRITE as RDS_RDMA_READWRITE,
        RDS_RDMA_SILENT as RDS_RDMA_SILENT,
        RDS_RDMA_USE_ONCE as RDS_RDMA_USE_ONCE,
        RDS_RECVERR as RDS_RECVERR,
        SO_VM_SOCKETS_BUFFER_MAX_SIZE as SO_VM_SOCKETS_BUFFER_MAX_SIZE,
        SO_VM_SOCKETS_BUFFER_MIN_SIZE as SO_VM_SOCKETS_BUFFER_MIN_SIZE,
        SO_VM_SOCKETS_BUFFER_SIZE as SO_VM_SOCKETS_BUFFER_SIZE,
        SOL_ALG as SOL_ALG,
        SOL_CAN_BASE as SOL_CAN_BASE,
        SOL_CAN_RAW as SOL_CAN_RAW,
        SOL_RDS as SOL_RDS,
        SOL_TIPC as SOL_TIPC,
        TIPC_ADDR_ID as TIPC_ADDR_ID,
        TIPC_ADDR_NAME as TIPC_ADDR_NAME,
        TIPC_ADDR_NAMESEQ as TIPC_ADDR_NAMESEQ,
        TIPC_CFG_SRV as TIPC_CFG_SRV,
        TIPC_CLUSTER_SCOPE as TIPC_CLUSTER_SCOPE,
        TIPC_CONN_TIMEOUT as TIPC_CONN_TIMEOUT,
        TIPC_CRITICAL_IMPORTANCE as TIPC_CRITICAL_IMPORTANCE,
        TIPC_DEST_DROPPABLE as TIPC_DEST_DROPPABLE,
        TIPC_HIGH_IMPORTANCE as TIPC_HIGH_IMPORTANCE,
        TIPC_IMPORTANCE as TIPC_IMPORTANCE,
        TIPC_LOW_IMPORTANCE as TIPC_LOW_IMPORTANCE,
        TIPC_MEDIUM_IMPORTANCE as TIPC_MEDIUM_IMPORTANCE,
        TIPC_NODE_SCOPE as TIPC_NODE_SCOPE,
        TIPC_PUBLISHED as TIPC_PUBLISHED,
        TIPC_SRC_DROPPABLE as TIPC_SRC_DROPPABLE,
        TIPC_SUB_CANCEL as TIPC_SUB_CANCEL,
        TIPC_SUB_PORTS as TIPC_SUB_PORTS,
        TIPC_SUB_SERVICE as TIPC_SUB_SERVICE,
        TIPC_SUBSCR_TIMEOUT as TIPC_SUBSCR_TIMEOUT,
        TIPC_TOP_SRV as TIPC_TOP_SRV,
        TIPC_WAIT_FOREVER as TIPC_WAIT_FOREVER,
        TIPC_WITHDRAWN as TIPC_WITHDRAWN,
        TIPC_ZONE_SCOPE as TIPC_ZONE_SCOPE,
        VM_SOCKETS_INVALID_VERSION as VM_SOCKETS_INVALID_VERSION,
        VMADDR_CID_ANY as VMADDR_CID_ANY,
        VMADDR_CID_HOST as VMADDR_CID_HOST,
        VMADDR_PORT_ANY as VMADDR_PORT_ANY,
    )

if sys.platform == "linux" and sys.version_info >= (3, 9):
    from _socket import (
        CAN_J1939 as CAN_J1939,
        CAN_RAW_JOIN_FILTERS as CAN_RAW_JOIN_FILTERS,
        J1939_EE_INFO_NONE as J1939_EE_INFO_NONE,
        J1939_EE_INFO_TX_ABORT as J1939_EE_INFO_TX_ABORT,
        J1939_FILTER_MAX as J1939_FILTER_MAX,
        J1939_IDLE_ADDR as J1939_IDLE_ADDR,
        J1939_MAX_UNICAST_ADDR as J1939_MAX_UNICAST_ADDR,
        J1939_NLA_BYTES_ACKED as J1939_NLA_BYTES_ACKED,
        J1939_NLA_PAD as J1939_NLA_PAD,
        J1939_NO_ADDR as J1939_NO_ADDR,
        J1939_NO_NAME as J1939_NO_NAME,
        J1939_NO_PGN as J1939_NO_PGN,
        J1939_PGN_ADDRESS_CLAIMED as J1939_PGN_ADDRESS_CLAIMED,
        J1939_PGN_ADDRESS_COMMANDED as J1939_PGN_ADDRESS_COMMANDED,
        J1939_PGN_MAX as J1939_PGN_MAX,
        J1939_PGN_PDU1_MAX as J1939_PGN_PDU1_MAX,
        J1939_PGN_REQUEST as J1939_PGN_REQUEST,
        SCM_J1939_DEST_ADDR as SCM_J1939_DEST_ADDR,
        SCM_J1939_DEST_NAME as SCM_J1939_DEST_NAME,
        SCM_J1939_ERRQUEUE as SCM_J1939_ERRQUEUE,
        SCM_J1939_PRIO as SCM_J1939_PRIO,
        SO_J1939_ERRQUEUE as SO_J1939_ERRQUEUE,
        SO_J1939_FILTER as SO_J1939_FILTER,
        SO_J1939_PROMISC as SO_J1939_PROMISC,
        SO_J1939_SEND_PRIO as SO_J1939_SEND_PRIO,
        UDPLITE_RECV_CSCOV as UDPLITE_RECV_CSCOV,
        UDPLITE_SEND_CSCOV as UDPLITE_SEND_CSCOV,
    )
if sys.platform == "linux" and sys.version_info >= (3, 10):
    from _socket import IPPROTO_MPTCP as IPPROTO_MPTCP
if sys.platform == "linux" and sys.version_info >= (3, 11):
    from _socket import SO_INCOMING_CPU as SO_INCOMING_CPU

if sys.version_info >= (3, 12):
    from _socket import (
        IP_ADD_SOURCE_MEMBERSHIP as IP_ADD_SOURCE_MEMBERSHIP,
        IP_BLOCK_SOURCE as IP_BLOCK_SOURCE,
        IP_DROP_SOURCE_MEMBERSHIP as IP_DROP_SOURCE_MEMBERSHIP,
        IP_PKTINFO as IP_PKTINFO,
        IP_UNBLOCK_SOURCE as IP_UNBLOCK_SOURCE,
    )

    if sys.platform == "win32":
        from _socket import (
            HV_GUID_BROADCAST as HV_GUID_BROADCAST,
            HV_GUID_CHILDREN as HV_GUID_CHILDREN,
            HV_GUID_LOOPBACK as HV_GUID_LOOPBACK,
            HV_GUID_PARENT as HV_GUID_PARENT,
            HV_GUID_WILDCARD as HV_GUID_WILDCARD,
            HV_GUID_ZERO as HV_GUID_ZERO,
            HV_PROTOCOL_RAW as HV_PROTOCOL_RAW,
            HVSOCKET_ADDRESS_FLAG_PASSTHRU as HVSOCKET_ADDRESS_FLAG_PASSTHRU,
            HVSOCKET_CONNECT_TIMEOUT as HVSOCKET_CONNECT_TIMEOUT,
            HVSOCKET_CONNECT_TIMEOUT_MAX as HVSOCKET_CONNECT_TIMEOUT_MAX,
            HVSOCKET_CONNECTED_SUSPEND as HVSOCKET_CONNECTED_SUSPEND,
        )
    else:
        from _socket import (
            ETHERTYPE_ARP as ETHERTYPE_ARP,
            ETHERTYPE_IP as ETHERTYPE_IP,
            ETHERTYPE_IPV6 as ETHERTYPE_IPV6,
            ETHERTYPE_VLAN as ETHERTYPE_VLAN,
        )

    if sys.platform == "linux":
        from _socket import ETH_P_ALL as ETH_P_ALL

    if sys.platform != "linux" and sys.platform != "win32" and sys.platform != "darwin":
        # FreeBSD >= 14.0
        from _socket import PF_DIVERT as PF_DIVERT

# Re-exported from errno
EBADF: int
EAGAIN: int
EWOULDBLOCK: int

# These errors are implemented in _socket at runtime
# but they consider themselves to live in socket so we'll put them here.
error = OSError

class herror(error): ...
class gaierror(error): ...

if sys.version_info >= (3, 10):
    timeout = TimeoutError
else:
    class timeout(error): ...

class AddressFamily(IntEnum):
    AF_INET = 2
    AF_INET6 = 10
    AF_APPLETALK = 5
    AF_DECnet = ...
    AF_IPX = 4
    AF_SNA = 22
    AF_UNSPEC = 0
    if sys.platform != "darwin":
        AF_IRDA = 23
    if sys.platform != "win32":
        AF_ROUTE = 16
        AF_SYSTEM = 32
        AF_UNIX = 1
    if sys.platform != "win32" and sys.platform != "darwin":
        AF_AAL5 = ...
        AF_ASH = 18
        AF_ATMPVC = 8
        AF_ATMSVC = 20
        AF_AX25 = 3
        AF_BRIDGE = 7
        AF_ECONET = 19
        AF_KEY = 15
        AF_LLC = 26
        AF_NETBEUI = 13
        AF_NETROM = 6
        AF_PPPOX = 24
        AF_ROSE = 11
        AF_SECURITY = 14
        AF_WANPIPE = 25
        AF_X25 = 9
    if sys.platform == "linux":
        AF_CAN = 29
        AF_PACKET = 17
        AF_RDS = 21
        AF_TIPC = 30
        AF_ALG = 38
        AF_NETLINK = 16
        AF_VSOCK = 40
        AF_QIPCRTR = 42
    if sys.platform != "win32" or sys.version_info >= (3, 9):
        AF_LINK = 33
        if sys.platform != "darwin":
            AF_BLUETOOTH = 32
    if sys.platform == "win32" and sys.version_info >= (3, 12):
        AF_HYPERV = 34
    if sys.platform != "linux" and sys.platform != "win32" and sys.platform != "darwin" and sys.version_info >= (3, 12):
        # FreeBSD >= 14.0
        AF_DIVERT = 44

AF_INET = AddressFamily.AF_INET
AF_INET6 = AddressFamily.AF_INET6
AF_APPLETALK = AddressFamily.AF_APPLETALK
AF_DECnet = AddressFamily.AF_DECnet
AF_IPX = AddressFamily.AF_IPX
AF_SNA = AddressFamily.AF_SNA
AF_UNSPEC = AddressFamily.AF_UNSPEC

if sys.platform != "darwin":
    AF_IRDA = AddressFamily.AF_IRDA

if sys.platform != "win32":
    AF_ROUTE = AddressFamily.AF_ROUTE
    AF_SYSTEM = AddressFamily.AF_SYSTEM
    AF_UNIX = AddressFamily.AF_UNIX

if sys.platform != "win32" and sys.platform != "darwin":
    AF_AAL5 = AddressFamily.AF_AAL5
    AF_ASH = AddressFamily.AF_ASH
    AF_ATMPVC = AddressFamily.AF_ATMPVC
    AF_ATMSVC = AddressFamily.AF_ATMSVC
    AF_AX25 = AddressFamily.AF_AX25
    AF_BRIDGE = AddressFamily.AF_BRIDGE
    AF_ECONET = AddressFamily.AF_ECONET
    AF_KEY = AddressFamily.AF_KEY
    AF_LLC = AddressFamily.AF_LLC
    AF_NETBEUI = AddressFamily.AF_NETBEUI
    AF_NETROM = AddressFamily.AF_NETROM
    AF_PPPOX = AddressFamily.AF_PPPOX
    AF_ROSE = AddressFamily.AF_ROSE
    AF_SECURITY = AddressFamily.AF_SECURITY
    AF_WANPIPE = AddressFamily.AF_WANPIPE
    AF_X25 = AddressFamily.AF_X25

if sys.platform == "linux":
    AF_CAN = AddressFamily.AF_CAN
    AF_PACKET = AddressFamily.AF_PACKET
    AF_RDS = AddressFamily.AF_RDS
    AF_TIPC = AddressFamily.AF_TIPC
    AF_ALG = AddressFamily.AF_ALG
    AF_NETLINK = AddressFamily.AF_NETLINK
    AF_VSOCK = AddressFamily.AF_VSOCK
    AF_QIPCRTR = AddressFamily.AF_QIPCRTR

if sys.platform != "win32" or sys.version_info >= (3, 9):
    AF_LINK = AddressFamily.AF_LINK
    if sys.platform != "darwin":
        AF_BLUETOOTH = AddressFamily.AF_BLUETOOTH

if sys.platform == "win32" and sys.version_info >= (3, 12):
    AF_HYPERV = AddressFamily.AF_HYPERV
if sys.platform != "linux" and sys.platform != "win32" and sys.platform != "darwin" and sys.version_info >= (3, 12):
    # FreeBSD >= 14.0
    AF_DIVERT = AddressFamily.AF_DIVERT

class SocketKind(IntEnum):
    SOCK_STREAM = 1
    SOCK_DGRAM = 2
    SOCK_RAW = 3
    SOCK_RDM = 4
    SOCK_SEQPACKET = 5
    if sys.platform == "linux":
        SOCK_CLOEXEC = 524288
        SOCK_NONBLOCK = 2048

SOCK_STREAM = SocketKind.SOCK_STREAM
SOCK_DGRAM = SocketKind.SOCK_DGRAM
SOCK_RAW = SocketKind.SOCK_RAW
SOCK_RDM = SocketKind.SOCK_RDM
SOCK_SEQPACKET = SocketKind.SOCK_SEQPACKET
if sys.platform == "linux":
    SOCK_CLOEXEC = SocketKind.SOCK_CLOEXEC
    SOCK_NONBLOCK = SocketKind.SOCK_NONBLOCK

class MsgFlag(IntFlag):
    MSG_CTRUNC = 8
    MSG_DONTROUTE = 4
    MSG_OOB = 1
    MSG_PEEK = 2
    MSG_TRUNC = 32
    MSG_WAITALL = 256

    if sys.platform != "darwin":
        MSG_BCAST = 1024
        MSG_MCAST = 2048
        MSG_ERRQUEUE = 8192

    if sys.platform != "win32" and sys.platform != "darwin":
        MSG_BTAG = ...
        MSG_CMSG_CLOEXEC = 1073741821
        MSG_CONFIRM = 2048
        MSG_ETAG = ...
        MSG_FASTOPEN = 536870912
        MSG_MORE = 32768
        MSG_NOTIFICATION = ...

    if sys.platform != "win32":
        MSG_DONTWAIT = 64
        MSG_EOF = 256
        MSG_EOR = 128
        MSG_NOSIGNAL = 16384  # sometimes this exists on darwin, sometimes not

MSG_CTRUNC = MsgFlag.MSG_CTRUNC
MSG_DONTROUTE = MsgFlag.MSG_DONTROUTE
MSG_OOB = MsgFlag.MSG_OOB
MSG_PEEK = MsgFlag.MSG_PEEK
MSG_TRUNC = MsgFlag.MSG_TRUNC
MSG_WAITALL = MsgFlag.MSG_WAITALL

if sys.platform != "darwin":
    MSG_BCAST = MsgFlag.MSG_BCAST
    MSG_MCAST = MsgFlag.MSG_MCAST
    MSG_ERRQUEUE = MsgFlag.MSG_ERRQUEUE

if sys.platform != "win32":
    MSG_DONTWAIT = MsgFlag.MSG_DONTWAIT
    MSG_EOF = MsgFlag.MSG_EOF
    MSG_EOR = MsgFlag.MSG_EOR
    MSG_NOSIGNAL = MsgFlag.MSG_NOSIGNAL  # Sometimes this exists on darwin, sometimes not

if sys.platform != "win32" and sys.platform != "darwin":
    MSG_BTAG = MsgFlag.MSG_BTAG
    MSG_CMSG_CLOEXEC = MsgFlag.MSG_CMSG_CLOEXEC
    MSG_CONFIRM = MsgFlag.MSG_CONFIRM
    MSG_ETAG = MsgFlag.MSG_ETAG
    MSG_FASTOPEN = MsgFlag.MSG_FASTOPEN
    MSG_MORE = MsgFlag.MSG_MORE
    MSG_NOTIFICATION = MsgFlag.MSG_NOTIFICATION

class AddressInfo(IntFlag):
    AI_ADDRCONFIG = 32
    AI_ALL = 16
    AI_CANONNAME = 2
    AI_NUMERICHOST = 4
    AI_NUMERICSERV = 1024
    AI_PASSIVE = 1
    AI_V4MAPPED = 8
    if sys.platform != "win32":
        AI_DEFAULT = 1536
        AI_MASK = 5127
        AI_V4MAPPED_CFG = 512

AI_ADDRCONFIG = AddressInfo.AI_ADDRCONFIG
AI_ALL = AddressInfo.AI_ALL
AI_CANONNAME = AddressInfo.AI_CANONNAME
AI_NUMERICHOST = AddressInfo.AI_NUMERICHOST
AI_NUMERICSERV = AddressInfo.AI_NUMERICSERV
AI_PASSIVE = AddressInfo.AI_PASSIVE
AI_V4MAPPED = AddressInfo.AI_V4MAPPED

if sys.platform != "win32":
    AI_DEFAULT = AddressInfo.AI_DEFAULT
    AI_MASK = AddressInfo.AI_MASK
    AI_V4MAPPED_CFG = AddressInfo.AI_V4MAPPED_CFG

if sys.platform == "win32":
    errorTab: dict[int, str]  # undocumented

class _SendableFile(Protocol):
    def read(self, size: int, /) -> bytes: ...
    def seek(self, offset: int, /) -> object: ...

    # optional fields:
    #
    # @property
    # def mode(self) -> str: ...
    # def fileno(self) -> int: ...

class socket(_socket.socket):
    def __init__(
        self, family: AddressFamily | int = -1, type: SocketKind | int = -1, proto: int = -1, fileno: int | None = None
    ) -> None: ...
    def __enter__(self) -> Self: ...
    def __exit__(self, *args: Unused) -> None: ...
    def dup(self) -> Self: ...  # noqa: F811
    def accept(self) -> tuple[socket, _RetAddress]: ...
    # Note that the makefile's documented windows-specific behavior is not represented
    # mode strings with duplicates are intentionally excluded
    @overload
    def makefile(
        self,
        mode: Literal["b", "rb", "br", "wb", "bw", "rwb", "rbw", "wrb", "wbr", "brw", "bwr"],
        buffering: Literal[0],
        *,
        encoding: str | None = None,
        errors: str | None = None,
        newline: str | None = None,
    ) -> SocketIO: ...
    @overload
    def makefile(
        self,
        mode: Literal["rwb", "rbw", "wrb", "wbr", "brw", "bwr"],
        buffering: Literal[-1, 1] | None = None,
        *,
        encoding: str | None = None,
        errors: str | None = None,
        newline: str | None = None,
    ) -> BufferedRWPair: ...
    @overload
    def makefile(
        self,
        mode: Literal["rb", "br"],
        buffering: Literal[-1, 1] | None = None,
        *,
        encoding: str | None = None,
        errors: str | None = None,
        newline: str | None = None,
    ) -> BufferedReader: ...
    @overload
    def makefile(
        self,
        mode: Literal["wb", "bw"],
        buffering: Literal[-1, 1] | None = None,
        *,
        encoding: str | None = None,
        errors: str | None = None,
        newline: str | None = None,
    ) -> BufferedWriter: ...
    @overload
    def makefile(
        self,
        mode: Literal["b", "rb", "br", "wb", "bw", "rwb", "rbw", "wrb", "wbr", "brw", "bwr"],
        buffering: int,
        *,
        encoding: str | None = None,
        errors: str | None = None,
        newline: str | None = None,
    ) -> IOBase: ...
    @overload
    def makefile(
        self,
        mode: Literal["r", "w", "rw", "wr", ""] = "r",
        buffering: int | None = None,
        *,
        encoding: str | None = None,
        errors: str | None = None,
        newline: str | None = None,
    ) -> TextIOWrapper: ...
    def sendfile(self, file: _SendableFile, offset: int = 0, count: int | None = None) -> int: ...
    @property
    def family(self) -> AddressFamily: ...
    @property
    def type(self) -> SocketKind: ...
    def get_inheritable(self) -> bool: ...
    def set_inheritable(self, inheritable: bool) -> None: ...

def fromfd(fd: SupportsIndex, family: AddressFamily | int, type: SocketKind | int, proto: int = 0) -> socket: ...

if sys.platform != "win32":
    if sys.version_info >= (3, 9):
        def send_fds(
            sock: socket, buffers: Iterable[ReadableBuffer], fds: Iterable[int], flags: Unused = 0, address: Unused = None
        ) -> int: ...
        def recv_fds(sock: socket, bufsize: int, maxfds: int, flags: int = 0) -> tuple[bytes, list[int], int, Any]: ...

if sys.platform == "win32":
    def fromshare(info: bytes) -> socket: ...

if sys.platform == "win32":
    def socketpair(family: int = ..., type: int = ..., proto: int = 0) -> tuple[socket, socket]: ...

else:
    def socketpair(
        family: int | AddressFamily | None = None, type: SocketType | int = ..., proto: int = 0
    ) -> tuple[socket, socket]: ...

class SocketIO(RawIOBase):
    def __init__(self, sock: socket, mode: Literal["r", "w", "rw", "rb", "wb", "rwb"]) -> None: ...
    def readinto(self, b: WriteableBuffer) -> int | None: ...
    def write(self, b: ReadableBuffer) -> int | None: ...
    @property
    def name(self) -> int: ...  # return value is really "int"
    @property
    def mode(self) -> Literal["rb", "wb", "rwb"]: ...

def getfqdn(name: str = "") -> str: ...

if sys.version_info >= (3, 11):
    def create_connection(
        address: tuple[str | None, int],
        timeout: float | None = ...,
        source_address: _Address | None = None,
        *,
        all_errors: bool = False,
    ) -> socket: ...

else:
    def create_connection(
        address: tuple[str | None, int], timeout: float | None = ..., source_address: _Address | None = None
    ) -> socket: ...

def has_dualstack_ipv6() -> bool: ...
def create_server(
    address: _Address, *, family: int = ..., backlog: int | None = None, reuse_port: bool = False, dualstack_ipv6: bool = False
) -> socket: ...

# the 5th tuple item is an address
def getaddrinfo(
    host: bytes | str | None, port: bytes | str | int | None, family: int = 0, type: int = 0, proto: int = 0, flags: int = 0
) -> list[tuple[AddressFamily, SocketKind, int, str, tuple[str, int] | tuple[str, int, int, int]]]: ...
