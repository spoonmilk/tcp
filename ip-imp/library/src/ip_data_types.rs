pub use crate::prelude::*;

#[derive (Debug)]
pub enum ForwardingOption {
    Ip(Ipv4Addr),
    Inter(Interface), // Forwarding to an interface
    ToSelf,           // For package destined for current node
}

#[derive (Debug)]
pub enum NodeType {
    Router,
    Host,
}

#[derive (Debug)]
pub struct Node {
    n_type: NodeType,
    interfaces: Vec<Interface>,
    forwarding_table: HashMap<Ipv4Net, ForwardingOption>,
}

impl Node {
    pub fn new(
        n_type: NodeType,
        interfaces: Vec<Interface>,
        forwarding_table: HashMap<Ipv4Net, ForwardingOption>,
    ) -> Node {
        Node {
            n_type,
            interfaces,
            forwarding_table,
        }
    }
    pub fn run() -> () {}
    fn check_packet(pack: Packet) -> bool {}
    fn forward_packet(pack: Packet) -> Result<()> {
        //Run it through check_packet to see if it should be dropped
        //Extract Destination Ip address
        //Run it through longest prefix
        //See what the value tied to that prefix is
        //If it's an Ip address, repeat with that IP address, an interface, forward via channel to that interface, if it is a ToSelf, handle internally
        //Return
    }
    fn longest_prefix(masks: Vec<Ipv4Net>, addr: Ipv4Addr) -> Ipv4Net {}
    fn process_packet(pack: Packet) -> () {}
}

#[derive(Debug, Clone, Copy)]
pub struct Interface {
    pub name: String,
    pub v_ip: Ipv4Addr,
    pub v_net: Ipv4Net,
    pub udp_addr: Ipv4Addr,
    pub udp_port: u16,
    pub neighbors: HashMap<Ipv4Addr, u16>,
}

impl Interface {
    pub fn new(
        name: &str,
        v_ip: Ipv4Addr,
        v_net: Ipv4Net,
        udp_addr: Ipv4Addr,
        udp_port: u16,
        neighbors: HashMap<Ipv4Addr, u16>,
    ) -> Interface {
        Interface {
            name: name.to_string(),
            v_ip,
            v_net,
            udp_addr,
            udp_port,
            neighbors,
        }
    }
    // TODO: READ ETHERPARSE DOCS, FIGURE OUT PACKET STRUCTURE
    pub fn send(pack: Packet) -> Result<()> {}
    pub fn recv() -> Result<Packet> {}
}

struct Packet {
    ttl: u8,
    protocol: u8,
    checksum: u16,
    src_addr: Ipv4Addr,
    dst_addr: Ipv4Addr,
    data: String
}
