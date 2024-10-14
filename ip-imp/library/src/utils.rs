use crate::prelude::*;
use std::{io::{Error, ErrorKind}, os::unix::net::SocketAddr};

/*
INCREDIBLY CONFUSING CHART OF FWDING TABLE STRUCTURE INTENDED TO MAKE SAID STRUCTURE LESS CONFUSING
Key: Ipv4Net, Val: Route(RouteType, Cost: Option<i32>, ForwardingOption)
                        ^(Local | Static | Rip  | ToSelf)   ^(Ip(Ipv4Addr) | Inter(String) | ToSelf)
                                                                                    ^Translate to InterRep via self.interface_reps hashmap
                                                                                                    ^(name:String, status:InterfaceStatus, neighbors:Vec<Ipv4Addr>, chan: BiChan<InterCmd, Packet>)
                                                                                                                            ^(Up | Down)                                    ^      ^(BuildSend(PacketBasis), Send(Packet), ToggleStatus)
                                                                                                                                                                            ^(Sender<T>, Receiver<U>)
 */

pub static INF: i32 = 16;

//Used as values of the forwarding table hashmap held by nodes
#[derive(Debug)]
pub struct Route {
    pub rtype: RouteType,           //Indicates how route was learned
    pub cost: Option<i32>, //Indicates cost of route (how many hops - important for lr REPL command) - cost can be unknown (for default route), hence the Option
    pub next_hop: ForwardingOption, //Contains all information needed to proceed with the routing process
}

impl Route {
    pub fn new(rtype: RouteType, cost: Option<i32>, next_hop: ForwardingOption) -> Route {
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
    Rip,    //Learned via RIP
    Static, //Static routes - default route is the only one I can think of under normal circumstances
    Local,  //Routes to local interfaces - routes with a forwardingOption of Inter
    ToSelf, //Routes where data is passed directly to the node - are not officially routes (I guess) and do not need to be printed out when lr REPL command is run
}

#[derive(Debug)]
pub enum ForwardingOption {
    Ip(Ipv4Addr),  //Forwarding to an IP address
    Inter(String), // Forwarding to an interface - the String is the name of the interface
    ToSelf,        // For package destined for current node
}

//Used to hold all the data that a node needs to know about a given interface
#[derive(Debug)]
pub struct InterfaceRep {
    pub name: String, //Interface name
    pub v_net: Ipv4Net,
    pub status: InterfaceStatus,         //Interface status
    pub neighbors: Vec<(Ipv4Addr, u16)>, //List of the interface's neighbors in (ipaddr, udpport) form
    pub chan: BiChan<InterCmd, Packet>, //Channel to send and receive messages from associated interface (sends InterCmd and receives Packet)
}

impl InterfaceRep {
    pub fn new(
        name: String,
        v_net: Ipv4Net,
        neighbors: Vec<(Ipv4Addr, u16)>,
        chan: BiChan<InterCmd, Packet>,
    ) -> InterfaceRep {
        InterfaceRep {
            name,
            v_net,
            status: InterfaceStatus::Up, //Status always starts as Up
            neighbors,
            chan,
        }
    }
    pub async fn command(&mut self, cmd: InterCmd) -> result::Result<(), mpsc::error::SendError<InterCmd>> {
        //Sends the input command to the interface
        self.chan.send.send(cmd).await
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
    Send(Packet, Ipv4Addr),           //Send this packet - when a packet is being forwarded
    ToggleStatus,                     //Make status down if up or up if down
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
        chan: BiChan<Packet, InterCmd>,
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
    pub async fn run(self) -> () {
        //Create mutexes to protect self
        let self_mutex1 = Arc::new(Mutex::new(self));
        let self_mutex2 = Arc::clone(&self_mutex1);
        //Listen for commands from the almighty node
        let mut node_listen = tokio::spawn(async move {
            loop {
                let mut slf = self_mutex1.lock().await;
                let chan_res = slf.chan.recv.recv().await;
                match chan_res {
                    Some(InterCmd::BuildSend(pb, next_hop)) => slf.send(slf.build(pb), next_hop).await.expect("Error sending packet"),
                    Some(InterCmd::Send(pack, next_hop)) => slf.send(pack, next_hop).await.expect("Error sending packet"),
                    Some(InterCmd::ToggleStatus) => slf.toggle_status(),
                    None => panic!("Channel to almight node disconnected :(")
                }
            }
        });
        //Listen for packets coming out of the ether-void
        let mut ether_listen = tokio::spawn(async move {
            loop {
                match self.status {
                    InterfaceStatus::Up => {
                        let mut slf = self_mutex2.lock().await;
                        let pack = slf.recv().await.expect("Error receiving packet");
                        self.pass_packet(pack).await.expect("Channel to almighty node disconnected");
                    },
                    InterfaceStatus::Down => tokio::task::yield_now().await //Avoids busy waiting
                }
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
    fn toggle_status(&mut self) -> () {
        match self.status {
            InterfaceStatus::Up => self.status = InterfaceStatus::Down,
            InterfaceStatus::Down => self.status = InterfaceStatus::Up
        }
    }
    fn build(&self, pb: PacketBasis) -> Vec<u8> {
        let src_ip = self.v_ip;
        let dst_ip = pb.dst_ip;
        let ttl = INF;
        let src_udp = self.udp_port;
        let dst_udp = self.neighbors.get(&dst_ip).unwrap().clone();
        let builder =
            PacketBuilder::ipv4(src_ip.octets(), dst_ip.octets(), ttl as u8).udp(src_udp, dst_udp);

        let payload: &[u8] = pb.msg.as_bytes();
        let mut result = Vec::<u8>::with_capacity(builder.size(payload.len()));

        //serialize
        builder.write(&mut result, &payload).unwrap();
        result
    }
    pub async fn send(&mut self, pack: Packet, next_hop: Ipv4Addr) -> io::Result<()> {
        let dst_neighbor = self.neighbors.get(&next_hop).unwrap();
        let bind_addr = format!("127.0.0.1:{}", self.udp_port);

        match UdpSocket::bind(bind_addr) {
            Ok(socket) => {
                match socket.send_to(&pack, format!("127.0.0.1:{}", dst_neighbor)) {
                    Ok(_) => Ok(()),
                    Err(e) => Err(e)
                }
            }
            Err(e) => Err(e)
        }
    }
    pub async fn recv(&mut self) -> Result<Packet> {
        match UdpSocket::bind(format!("127.0.0.1:{}", self.udp_port)) {
            Ok(socket) => {
                let mut buf: [u8; ] = [0 ; 60];



                return Ok(pack)
            }
            Err(e) => Err(e)
        }
    }
    async fn pass_packet(&mut self, pack: Packet) -> result::Result<(), mpsc::error::SendError<Packet>> {
        self.chan.send.send(pack).await
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
                *node = next_node.clone();
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
            None => Err(Error::new(
                ErrorKind::Other,
                "Could not construct a valid route match",
            )),
        }
    }
}
