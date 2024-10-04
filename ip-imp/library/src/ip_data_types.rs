#[path = "prelude.rs"]
pub mod prelude;

pub mod ip_data_types {
    use std::{net::Ipv4Addr, os::unix::net::SocketAddr};
 
    pub enum ForwardingOption {
        Ip(Ipv4Addr), // Forwarding directly to an IP address
        Inter(Interface), // Forwarding to an interface
        ToSelf // For package destined for current node
    }
    
    pub enum NodeType {
        Router,
        Host
    }
    
    #[derive(Debug, PrettyPrint)]
    pub struct Node {
        n_type: NodeType,
        interfaces: Vec<Interface>,
        forwarding_table: HashMap<Ipv4Network, ForwardingOption>
    }

    impl Node {
       pub fn new(n_type: NodeType, interfaces: Vec<Interface>, forwarding_table: HashMap<Ipv4Network, ForwardingOption>) -> Node {
         Node { n_type, interfaces, forwarding_table }
       }
       fn check_packet(pack: Packet) -> bool {}
       pub fn forward_packet(pack: Packet) -> Result<()> {
        //Run it through check_packet to see if it should be dropped 
        //Extract Destination Ip address
        //Run it through longest prefix
        //See what the value tied to that prefix is
        //If it's an Ip address, repeat with that IP address, an interface, forward via channel to that interface, if it is a ToSelf, handle internally
        //Return
       }
       fn longest_prefix(masks: Vec<Ipv4Network>, addr: Ipv4Addr) -> Ipv4Network {}
       pub fn process_packet(pack: Packet) -> () {} 
    }
    
    #[derive(Debug, PrettyPrint)]
    pub struct Interface {
        name: String,
        v_ip: Ipv4Addr,
        udp_sock: UdpSocket,
        neighbors: HashSet<Ipv4Addr>
    }

    impl Interface {
        pub fn new(name: &str, v_ip: Ipv4Addr, udp_addr: &str, neighbors: HashSet<Ipv4Addr>) -> Node {
            Interface { name: name.to_string(), v_ip, udp_sock: SocketAddr::from(udp_sock), neighbors }
          }
        // TODO: READ ETHERPARSE DOCS, FIGURE OUT PACKET STRUCTURE
        pub fn send(pack: Packet) -> Result<()> {} 
        pub fn recv() -> Result<Packet> {}
    }

    struct Packet {
        //DEFINED EXTERNALLY
    }
}