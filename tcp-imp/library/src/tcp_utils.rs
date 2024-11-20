use crate::prelude::*;

#[derive(Debug, Clone)]
pub enum TcpState {
    Listening, // Listener Socket constant state
    AwaitingRun, // Connection sockets upon creation, waiting to run
    Initialized, //Connection socket initialized by listener before processing SYN packet
    SynSent, // Connection Socket after SYN, waiting for SYN/ACK
    SynRecvd, // Connection Socket state after receiving a SYN, should respond SYN/ACK
    Established, //TCP handshake complete - now both parties can send data
    // Teardown things
    FinWait1,
    FinWait2,
    Closing, 
    TimeWait,
    CloseWait,
    LastAck,
    Closed
}

#[derive(Debug, Clone)]
pub struct TcpAddress {
    pub ip: Ipv4Addr,
    pub port: u16
}


impl TcpAddress {
    pub fn new(ip: Ipv4Addr, port: u16) -> TcpAddress {
        TcpAddress { ip, port }
    }
}



/// A TCP Packet with a TCP header and a payload ; Encapsulated in IP upon send
#[derive(Debug, Clone)]
pub struct TcpPacket {
    pub header: TcpHeader,
    pub payload: Vec<u8>
}

impl TcpPacket {
    pub fn new(header: TcpHeader, payload: Vec<u8>) -> Self {
        Self {
            header,
            payload
        }
    }
}

/// Serializes a TcpPacket into the PAYLOAD of an IP packet
pub fn serialize_tcp(packet: TcpPacket) -> Vec<u8> {
    let mut buffer = Vec::new();
    buffer.extend_from_slice(&packet.header.to_bytes());
    buffer.extend_from_slice(&packet.payload);
    buffer
}


/// Deserializes a TCP Packet from a Vec<u8> containing a WHOLE IP packet ; do not pass in payloads
pub fn deserialize_tcp(raw_packet: Vec<u8>) -> Result<TcpPacket> {
    match TcpHeader::from_slice(&raw_packet) {
        Ok((header, payload)) => {
            return Ok(TcpPacket {
                header,
                payload: payload.to_vec()
            });
        }
        Err(_) => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Failed to read received packet error",
            ));
        }
    }
}

// TCP FLAGS    
pub const URG: u8 = 1;
pub const ACK: u8 = 2;
pub const PSH: u8 = 4;
pub const RST: u8 = 8;
pub const SYN: u8 = 16;
pub const FIN: u8 = 32;

/// Takes a TCP header and returns the flags as a u8
pub fn header_flags(head: &TcpHeader) -> u8 {
    let bools = [false, false, head.fin, head.syn, head.rst, head.psh, head.ack, head.urg];
    bools.iter()
        .enumerate()
        .fold(0, |acc, (i, &b)| acc | ((b as u8) << (7 - i)))
}

/// Checks if a TCP header has only the specified flags
pub fn has_only_flags(head: &TcpHeader, flags: u8) -> bool {
    let head_flags = header_flags(&head);
    // The better/only way of checking equality
    (head_flags ^ flags) == 0
}

/// Checks if a TCP header has the specified flags, ignoring other flags
pub fn has_flags(head: &TcpHeader, flags: u8) -> bool {
    let head_flags = header_flags(&head);
    (head_flags & flags) == flags
}

/// Horrible terrible function to determine if a packet is SYN and ONLY SYN
pub fn is_syn(head: &TcpHeader) -> bool {
    if head.ns | head.fin | head.rst | head.psh | head.ack | head.urg | head.ece | head.cwr {
        return false
    } else if head.syn {
        true
    } else {
        false
    }
}

/*
/// Commands to the socket manager ; not the sockets themselves
pub enum SockMand { 
    Listen(u16), // Creates a listener socket on <port>
    Accept(u16), // Puts an existing listener socket on <port> in accepting state
    Connect(Ipv4Addr, u16) // Creates a connection socket to <ip> on <port>
    //More to come
}


/// Commands to the sockets ; not the socket manager
pub enum SocketCmd {
    Process(TcpPacket), //Process this TcpPacket - it's from your client
    Send(TcpPacket), //Send this TcpPacket to your client
    Recv(usize), //Give me usize many bytes of data that you've recieved
    Close //Tear down your connection
    //More perhaps
}
*/