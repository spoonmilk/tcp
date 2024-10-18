use crate::{prelude::*, rip_utils::RipMsg};
use std::io::{Error, ErrorKind};
use std::net::UdpSocket;

/*
INCREDIBLY CONFUSING CHART OF FWDING TABLE STRUCTURE INTENDED TO MAKE SAID STRUCTURE LESS CONFUSING
Key: Ipv4Net, Val: Route(RouteType, Cost: Option<i32>, ForwardingOption)
                        ^(Local | Static | Rip  | ToSelf)   ^(Ip(Ipv4Addr) | Inter(String) | ToSelf)
                                                                                    ^Translate to InterRep via self.interface_reps hashmap
                                                                                                    ^(name:String, status:InterfaceStatus, neighbors:Vec<Ipv4Addr>, chan: BiChan<InterCmd, Packet>)
                                                                                                                            ^(Up | Down)                                    ^      ^(BuildSend(PacketBasis), Send(Packet), ToggleStatus)
                                                                                                                                                                            ^(Sender<T>, Receiver<U>)
*/

//Used as values of the forwarding table hashmap held by nodes
#[derive(Debug)]
pub struct Route {
    pub rtype: RouteType, //Indicates how route was learned
    pub cost: Option<u32>, //Indicates cost of route (how many hops - important for lr REPL command) - cost can be unknown (for default route), hence the Option
    pub next_hop: ForwardingOption, //Contains all information needed to proceed with the routing process
}

impl Route {
    pub fn new(rtype: RouteType, cost: Option<u32>, next_hop: ForwardingOption) -> Route {
        Route {
            rtype,
            cost,
            next_hop,
        }
    }
}

//Used to indicate how a route was learned by the router (important for lr REPL command)
#[derive(Debug)]
pub enum RouteType {
    Rip, //Learned via RIP
    Static, //Static routes - default route is the only one I can think of under normal circumstances
    Local, //Routes to local interfaces - routes with a forwardingOption of Inter
    ToSelf, //Routes where data is passed directly to the node - are not officially routes (I guess) and do not need to be printed out when lr REPL command is run
}

#[derive(Debug, PartialEq)]
pub enum ForwardingOption {
    Ip(Ipv4Addr), //Forwarding to an IP address
    Inter(String), // Forwarding to an interface - the String is the name of the interface
    ToSelf, // For package destined for current node
}

//Used to hold all the data that a node needs to know about a given interface
#[derive(Debug)]
pub struct InterfaceRep {
    pub name: String, //Interface name
    pub v_net: Ipv4Net,
    pub status: InterfaceStatus, //Interface status
    pub neighbors: Vec<(Ipv4Addr, u16)>, //List of the interface's neighbors in (ipaddr, udpport) form
    pub chan: BiChan<InterCmd, Packet>, //Channel to send and receive messages from associated interface (sends InterCmd and receives Packet)
}

impl InterfaceRep {
    pub fn new(
        name: String,
        v_net: Ipv4Net,
        neighbors: Vec<(Ipv4Addr, u16)>,
        chan: BiChan<InterCmd, Packet>
    ) -> InterfaceRep {
        InterfaceRep {
            name,
            v_net,
            status: InterfaceStatus::Up, //Status always starts as Up
            neighbors,
            chan,
        }
    }
    pub fn command(&mut self, cmd: InterCmd) -> result::Result<(), SendError<InterCmd>> {
        //Sends the input command to the interface
        self.chan.send.send(cmd)
    }
}

//Used to indicate if an Interface is down or up
#[derive(Debug)]
pub enum InterfaceStatus {
    Up,
    Down,
}

//Used for messages that a node sends to an interface
#[derive(Debug)]
pub enum InterCmd {
    BuildSend(PacketBasis, Ipv4Addr), //Build a packet using this PacketBasis and send it - when a send REPL command is used
    Send(Packet, Ipv4Addr), //Send this packet - when a packet is being forwarded
    ToggleStatus, //Make status down if up or up if down
}

//Used to store the data an interface needs to build a packet and send it
#[derive(Debug)]
pub struct PacketBasis {
    pub dst_ip: Ipv4Addr,
    pub msg: String,
}

#[derive(Debug)]
pub struct Interface {
    pub name: String,
    pub v_ip: Ipv4Addr,
    pub v_net: Ipv4Net,
    pub udp_addr: Ipv4Addr,
    pub udp_port: u16,
    pub neighbors: HashMap<Ipv4Addr, u16>,
    pub chan: BiChan<Packet, InterCmd>, //Channel that sends packets and receives InterCmd
    pub status: InterfaceStatus, //Only non-static field - represents current status of the interface
}

impl Interface {
    pub fn new(
        name: String,
        v_ip: Ipv4Addr,
        v_net: Ipv4Net,
        udp_addr: Ipv4Addr,
        udp_port: u16,
        neighbors: HashMap<Ipv4Addr, u16>,
        chan: BiChan<Packet, InterCmd>
    ) -> Interface {
        Interface {
            name,
            v_ip,
            v_net,
            udp_addr,
            udp_port,
            neighbors,
            chan,
            status: InterfaceStatus::Up, //Status always starts as Up
        }
    }
    pub fn run(self) -> () {
        //Create mutexes to protect self
        let self_mutex1 = Arc::new(Mutex::new(self));
        let self_mutex2 = Arc::clone(&self_mutex1);

        //Listen for commands from the almighty node
        thread::spawn(move || Interface::node_listen(self_mutex1));
        //Listen for packets coming out of the ether-void
        Interface::ether_listen(self_mutex2);
    }
    fn node_listen(slf_mutex: Arc<Mutex<Interface>>) -> () {
        let mut slf = slf_mutex.lock().unwrap();
        loop { 
            let chan_res = slf.chan.recv.recv();
            match chan_res {
                Ok(InterCmd::BuildSend(pb, next_hop)) => { 
                    let builded = slf.build(pb);
                    slf.send(builded, next_hop).expect("Error sending packet") 
                }
                Ok(InterCmd::Send(pack, next_hop)) => slf.send(pack, next_hop).expect("Error sending packet"),
                Ok(InterCmd::ToggleStatus) => slf.toggle_status(),
                Err(e) => panic!("Error receiving on channel: {e:?}")
            }
        }
    } 
    fn ether_listen(slf_mutex: Arc<Mutex<Interface>>) -> () {
        let mut slf = slf_mutex.lock().unwrap();
        loop {
            match slf.status {
                InterfaceStatus::Up => { 
                    let pack = slf.recv().expect("Error receiving packet");
                    slf.pass_packet(pack).expect("Channel to almighty node disconnected");
                },
                InterfaceStatus::Down => {} //Avoids busy waiting
            }
        }
    }
    fn toggle_status(&mut self) -> () {
        match self.status {
            InterfaceStatus::Up => {
                self.status = InterfaceStatus::Down;
            }
            InterfaceStatus::Down => {
                self.status = InterfaceStatus::Up;
            }
        }
    }
    fn build(&mut self, pb: PacketBasis) -> Packet {
        // TODO: ADD BUILDING RIP PACKETS   




        // Grabbing info from sending interface for header
        let src_ip = self.v_ip;
        let dst_ip = pb.dst_ip;
        let ttl: u8 = 16; // Default TTL from handout
        // Instantiate payload
        let payload: Vec<u8> = Vec::from(pb.msg.as_bytes());
        // Create the header
        let mut header = Ipv4Header {
            source: src_ip.octets(),
            destination: dst_ip.octets(),
            time_to_live: ttl,
            total_len: Ipv4Header::MIN_LEN_U16 + (pb.msg.len() as u16),
            protocol: IpNumber::UDP,
            ..Default::default()
        };
        // Checksum
        header.header_checksum = header.calc_header_checksum();
        return Packet { header, data: payload }; // Packet built!
    }
    pub fn send(&mut self, pack: Packet, next_hop: Ipv4Addr) -> std::io::Result<()> {
        // Grab neighbor address to send to
        let dst_neighbor = self.neighbors.get(&next_hop).unwrap();
        // Self address for binding
        let bind_addr = format!("127.0.0.1:{}", self.udp_port);
        // Bind to Udp port and attempt to send to neighbor
        let sock = UdpSocket::bind(bind_addr)?;

        match sock.send_to(&pack.data, format!("127.0.0.1:{}", dst_neighbor)) {
            // TODO: Do something on Ok? Make error more descriptive?
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
    pub fn recv(&mut self) -> Result<Packet> {
        let mut received = false;
        match UdpSocket::bind(format!("127.0.0.1:{}", self.udp_port)) {
            Ok(socket) => {
                let mut buf: [u8; 40] = [0; 40];
                while !received {
                    let len = socket.recv(&mut buf)?; // Break if receive
                    if len != 0 {
                        received = !received;
                    }
                }
                match Ipv4Header::from_slice(&buf) {
                    Ok((head, rest)) => {
                        let len = (head.total_len - 20) as usize;
                        let pay: Vec<u8> = Vec::from_iter(rest[0..len].iter().cloned());
                        return Ok(Packet { header: head, data: pay });
                    }
                    Err(_) => {
                        return Err(
                            Error::new(
                                ErrorKind::InvalidData,
                                "Failed to read received packet error"
                            )
                        );
                    }
                }
            }
            Err(e) => Err(e),
        }
    }
    fn pass_packet(&mut self, pack: Packet) -> result::Result<(), SendError<Packet>> {
        self.chan.send.send(pack) 
    }
}

#[derive(Debug, Clone)]
pub struct Packet {
    pub header: Ipv4Header,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct BiChan<T, U> {
    pub send: Sender<T>,
    pub recv: Receiver<U>,
}

/// Structs for forwarding table longest prefix match implementation
#[derive(Default, Debug, Clone)]
pub struct TrieNode {
    is_end: bool, // Utility boolean, true for end of IP/start of mask
    children: HashMap<u8, TrieNode>,
    route: String,
}

impl TrieNode {
    pub fn new() -> TrieNode {
        TrieNode::default()
    }

    pub fn insert(&mut self, net_addr: Ipv4Addr) {
        // Get mutable reference to root node
        let mut node = self;
        // Get octets from netAddr
        let net_addr_octets = net_addr.octets();

        for oct in net_addr_octets {
            node = node.children.entry(oct).or_default();
            node.route.push_str(oct.to_string().as_str());
        }
        node.is_end = true;
    }

    pub fn search(&mut self, dst: &Ipv4Addr) -> Result<String> {
        let node = self;
        let dst_oct = dst.octets(); // Get destination address octets
        // Keep track of last available route
        let mut last_route: Option<String> = None;

        for oct in dst_oct {
            if let Some(next_node) = node.children.get(&oct) {
                let node = next_node.clone();
                if node.is_end {
                    last_route = Some(node.route.clone());
                }
            } else {
                break;
            }
        }

        match last_route {
            Some(route) => Ok(route),
            // Error fuckery
            None => Err(Error::new(ErrorKind::Other, "Could not construct a valid route match")),
        }
    }
}
