use crate::prelude::*;
use std::io::{Error, ErrorKind};
use std::time::Instant;

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
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub rtype: RouteType,           //Indicates how route was learned
    pub cost: Option<u32>, //Indicates cost of route (how many hops - important for lr REPL command) - cost can be unknown (for default route), hence the Option
    pub next_hop: ForwardingOption, //Contains all information needed to proceed with the routing process
    pub creation_time: u64,
}

impl Route {
    pub fn new(rtype: RouteType, cost: Option<u32>, next_hop: ForwardingOption) -> Route {
        Route {
            rtype,
            cost,
            next_hop,
            creation_time: Instant::now().elapsed().as_millis() as u64,
        }
    }
}

//Used to indicate how a route was learned by the router (important for lr REPL command)
#[derive(Debug, Clone, PartialEq)]
pub enum RouteType {
    Rip,    //Learned via RIP
    Static, //Static routes - default route is the only one I can think of under normal circumstances
    Local,  //Routes to local interfaces - routes with a forwardingOption of Inter
    ToSelf, //Routes where data is passed directly to the node - are not officially routes (I guess) and do not need to be printed out when lr REPL command is run
}

#[derive(Debug, PartialEq, Clone)]
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
    pub v_ip: Ipv4Addr,
    pub status: InterfaceStatus,         //Interface status
    pub neighbors: Vec<(Ipv4Addr, u16)>, //List of the interface's neighbors in (ipaddr, udpport) form
    pub chan: BiChan<InterCmd, Packet>, //Channel to send and receive messages from associated interface (sends InterCmd and receives Packet)
}

impl InterfaceRep {
    pub fn new(
        name: String,
        v_net: Ipv4Net,
        v_ip: Ipv4Addr,
        neighbors: Vec<(Ipv4Addr, u16)>,
        chan: BiChan<InterCmd, Packet>,
    ) -> InterfaceRep {
        InterfaceRep {
            name,
            v_net,
            v_ip,
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
    BuildSend(PacketBasis, Ipv4Addr, bool), //Build a packet using this PacketBasis and send it - when a send REPL command is used
    Send(Packet, Ipv4Addr),                 //Send this packet - when a packet is being forwarded
    ToggleStatus,                           //Make status down if up or up if down
}

//Used to store the data an interface needs to build a packet and send it
#[derive(Debug, Clone)]
pub struct PacketBasis {
    pub dst_ip: Ipv4Addr,
    pub msg: Vec<u8>,
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
    pub fn run(mut self) -> () {
        let mut udp_sock = UdpSocket::bind(format!("127.0.0.1:{}", self.udp_port))
            .expect("Unable to bind to port");
        udp_sock.set_nonblocking(true).unwrap();
        loop {
            self.node_listen(&mut udp_sock);
            self.ether_listen(&mut udp_sock);
        }
    }
    fn node_listen(&mut self, udp_sock: &mut UdpSocket) -> () {
        let chan_res = self.chan.recv.try_recv();
        match chan_res {
            Ok(InterCmd::BuildSend(pb, next_hop, msg_type)) => {
                if let InterfaceStatus::Up = self.status {
                    let builded = self.build(pb, msg_type);
                    self.send(udp_sock, builded, next_hop)
                        .expect("Error sending packet");
                }
            }
            Ok(InterCmd::Send(pack, next_hop)) => {
                if let InterfaceStatus::Up = self.status {
                    self.send(udp_sock, pack, next_hop)
                        .expect("Error sending packet");
                }
            }
            Ok(InterCmd::ToggleStatus) => self.toggle_status(),
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => panic!("Channel to node disconnected :("),
        }
    }
    fn ether_listen(&self, udp_sock: &mut UdpSocket) -> () {
        match self.status {
            InterfaceStatus::Up => {
                let pack = match self.try_recv(udp_sock) {
                    Ok(pack) => pack,
                    Err(ref e) if e.kind() == ErrorKind::WouldBlock => return,
                    Err(e) => panic!("Error while trying to recv: {e:?}"),
                };
                self.pass_packet(pack)
                    .expect("Channel to almighty node disconnected");
            }
            InterfaceStatus::Down => {}
        }
    }
    //Fix this upon failure
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
    fn build(&self, pb: PacketBasis, msg_type: bool) -> Packet {
        // Grabbing info from sending interface for header
        let src_ip = self.v_ip;
        let dst_ip = pb.dst_ip;
        let ttl: u8 = 16; // Default TTL from handout
                          // Instantiate payload
        let payload: Vec<u8> = pb.msg;
        let prot_num: IpNumber = if msg_type { 0.into() } else { 200.into() };
        // Create the header
        let mut header = Ipv4Header {
            source: src_ip.octets(),
            destination: dst_ip.octets(),
            time_to_live: ttl,
            total_len: Ipv4Header::MIN_LEN_U16 + (payload.len() as u16),
            protocol: prot_num,
            ..Default::default()
        };
        // Checksum
        header.header_checksum = header.calc_header_checksum();
        return Packet {
            header,
            data: payload,
        }; // Packet built!
    }
    pub fn send(
        &self,
        sock: &mut UdpSocket,
        pack: Packet,
        next_hop: Ipv4Addr,
    ) -> std::io::Result<()> {
        // Grab neighbor address to send to
        println!("My neighbors are: {:#?}", self.neighbors);
        println!("Next hop is: {}", next_hop);
        let dst_neighbor = self.neighbors.get(&next_hop).unwrap();
        let mut message = vec![0u8; 20];
        let mut writer = &mut message[..];
        pack.header.write(&mut writer)?;
        message.extend(pack.data);

        // Send
        match sock.send_to(&message, format!("127.0.0.1:{}", dst_neighbor)) {
            // TODO: Do something on Ok? Make error more descriptive?
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
    pub fn try_recv(&self, socket: &mut UdpSocket) -> Result<Packet> {
        let mut received = false;
        let mut buf: [u8; 1500] = [0; 1500];
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
                return Ok(Packet {
                    header: head,
                    data: pay,
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
    fn pass_packet(&self, pack: Packet) -> result::Result<(), SendError<Packet>> {
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

