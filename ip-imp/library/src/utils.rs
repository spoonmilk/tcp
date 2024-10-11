use crate::prelude::*;


#[derive (Debug)]
pub enum ForwardingOption {
    Ip(Ipv4Addr),
    Inter(Interface), // Forwarding to an interface
    ToSelf,           // For package destined for current node
}

#[derive(Debug, Clone)]
pub struct Interface {
    pub name: String,
    pub v_ip: Ipv4Addr,
    pub v_net: Ipv4Net,
    pub udp_addr: Ipv4Addr,
    pub udp_port: u16,
    //pub chan: BiChan<Packet>,
    pub neighbors: HashMap<Ipv4Addr, u16>,
}

impl Interface {
    pub fn new(
        name: &str,
        v_ip: Ipv4Addr,
        v_net: Ipv4Net,
        udp_addr: Ipv4Addr,
        udp_port: u16,
        //chan: BiChan<Packet>,
        neighbors: HashMap<Ipv4Addr, u16>,
    ) -> Interface {
        Interface {
            name: name.to_string(),
            v_ip,
            v_net,
            udp_addr,
            udp_port,
            //chan,
            neighbors,
        }
    }
    pub async fn run(self) -> () {
        //Listen for commands from the almighty node
        let mut node_listen = tokio::spawn(async {
            loop {
                println!("LISTENING TO NODE");
            }
        });
        //Listen for packets coming out of the ether-void
        let mut ether_listen = tokio::spawn(async {
            loop {
                println!("LISTENING FOR MESSAGES FROM THE VOID");
            }
        });
        //Switch betwen listening for node commands or ether packets
        loop {
            tokio::select! {
                _ = &mut node_listen => {},
                _ = &mut ether_listen => {}
            }
        }
    }
    // TODO: READ ETHERPARSE DOCS, FIGURE OUT PACKET STRUCTURE
    //pub fn send(pack: Packet) -> Result<()> {}
    //pub fn recv() -> Result<Packet> {}
}

#[derive (Debug)]
pub struct Packet {
    ttl: u8,
    protocol: u8,
    checksum: u16,
    src_addr: Ipv4Addr,
    dst_addr: Ipv4Addr,
    data: String
}

#[derive (Debug)]
pub struct BiChan<T> {
    pub send: Sender<T>,
    pub recv: Receiver<T>
}