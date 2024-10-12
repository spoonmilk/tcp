use crate::prelude::*;

/*
INCREDIBLY CONFUSING CHART OF FWDING TABLE STRUCTURE INTENDED TO MAKE SAID STRUCTURE LESS CONFUSING
Key: Ipv4Net, Val: Route(RouteType, Cost: Option<i32>, ForwardingOption)
                        ^(Local | Static | Rip  | ToSelf)   ^(Ip(Ipv4Addr) | Inter(String) | ToSelf)
                                                                                    ^Translate to InterRep via self.interface_reps hashmap
                                                                                                    ^(name:String, status:InterfaceStatus, neighbors:Vec<Ipv4Addr>, chan: BiChan<InterCmd, Packet>)
                                                                                                                            ^(Up | Down)                                    ^      ^(Send(Packet), ToggleStatus)
                                                                                                                                                                            ^(Sender<T>, Receiver<U>)
 */


//Used as values of the forwarding table hashmap held by nodes
#[derive (Debug)]
pub struct Route {
    rtype: RouteType, //Indicates how route was learned
    cost: Option<i32>, //Indicates cost of route (how many hops - important for lr REPL command) - cost can be unknown (for default route), hence the Option
    next_hop: ForwardingOption //Contains all information needed to proceed with the routing process
}

impl Route {
    pub fn new(rtype: RouteType, cost: Option<i32>, next_hop: ForwardingOption) -> Route {
        Route {rtype, cost, next_hop}
    }
}

//Used to indicate how a route was learned by the router (important for lr REPL command)
#[derive (Debug)]
pub enum RouteType {
    Rip, //Learned via RIP
    Static, //Static routes - default route is the only one I can think of under normal circumstances
    Local, //Routes to local interfaces - routes with a forwardingOption of Inter
    ToSelf //Routes where data is passed directly to the node - are not officially routes (I guess) and do not need to be printed out when lr REPL command is run
}

#[derive (Debug)]
pub enum ForwardingOption {
    Ip(Ipv4Addr), //Forwarding to an IP address
    Inter(String), // Forwarding to an interface - the String is the name of the interface
    ToSelf,           // For package destined for current node
}

//Used to hold all the data that a node needs to know about a given interface
#[derive (Debug)]
pub struct InterfaceRep { 
    pub name: String, //Interface name
    pub status: InterfaceStatus, //Interface status
    pub neighbors: Vec<Ipv4Addr>, //List of the interface's nieghbors 
    pub chan: BiChan<InterCmd, Packet>, //Channel to send and receive messages from associated interface (sends InterCmd and receives Packet)
}

impl InterfaceRep {
    pub fn new(name: String, neighbors: Vec<Ipv4Addr>, chan: BiChan<InterCmd, Packet>) -> InterfaceRep {
        InterfaceRep {
            name, 
            status: InterfaceStatus::Up, //Status always starts as Up
            neighbors, 
            chan
        }
    }
    fn toggle_status() -> Result<()> {}, //Tells interface to toggle its status
    fn send_packet(pack: Packet) -> Result<()> {}, //Tells interface to send a packet
}

//Used to indicate if an Interface is down or up
#[derive (Debug)]
pub enum InterfaceStatus {
    Up,
    Down
}

//Used for messages that a node sends to an interface
#[derive (Debug)]
pub enum InterCmd {
    Send(Packet), //Send this packet
    ToggleStatus //Make status down if up or up if down
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
            status: InterfaceStatus::Up //Status always starts as Up
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
    pub ttl: u8,
    pub protocol: u8,
    pub checksum: u16,
    pub src_addr: Ipv4Addr,
    pub dst_addr: Ipv4Addr,
    pub data: String
}

#[derive (Debug)]
pub struct BiChan<T,U> {
    pub send: Sender<T>,
    pub recv: Receiver<U>
}