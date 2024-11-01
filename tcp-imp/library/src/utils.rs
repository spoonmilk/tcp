use crate::prelude::*;

pub type ForwardingTable = HashMap<Ipv4Net, Route>;
pub type InterfaceTable = HashMap<String, InterfaceRep>; //Is shared via Arc<RwLock<>>
pub type InterfaceRecvers = HashMap<String, Receiver<Packet>>; //NEVER shared - only IPDaemon has this
pub type RipNeighbors = HashMap<Ipv4Addr, Vec<Route>>; 
//type SocketTable = ...?

//Used as values of the forwarding table hashmap held by nodes
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub rtype: RouteType,           //Indicates how route was learned
    pub cost: Option<u32>, //Indicates cost of route (how many hops - important for lr REPL command) - cost can be unknown (for default route), hence the Option
    pub next_hop: ForwardingOption, //Contains all information needed to proceed with the routing process
    pub creation_time: Instant,
}

impl Route {
    pub fn new(rtype: RouteType, cost: Option<u32>, next_hop: ForwardingOption) -> Route {
        Route {
            rtype,
            cost,
            next_hop,
            creation_time: Instant::now(),
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

impl<T, U> BiChan<T, U> {
    pub fn make_bichans() -> (BiChan<T, U>, BiChan<U, T>) {
        let chan1 = channel::<T>();
        let chan2 = channel::<U>();
        let inter_chan = BiChan {
            send: chan1.0,
            recv: chan2.1,
        };
        let inter_rep_chan = BiChan {
            send: chan2.0,
            recv: chan1.1,
        };
        (inter_chan, inter_rep_chan)
    }
}

#[derive(Debug)]
pub struct InterfaceRep {
    pub name: String, //Interface name
    pub v_net: Ipv4Net,
    pub v_ip: Ipv4Addr,
    pub status: InterfaceStatus,         //Interface status
    pub neighbors: Vec<(Ipv4Addr, u16)>, //List of the interface's neighbors in (ipaddr, udpport) form
    pub sender: Sender<InterCmd>, //Channel to send messages from associated interface (sends InterCmd and receives Packet)
}

impl InterfaceRep {
    pub fn new(
        name: String,
        v_net: Ipv4Net,
        v_ip: Ipv4Addr,
        neighbors: Vec<(Ipv4Addr, u16)>,
        sender: Sender<InterCmd>,
    ) -> InterfaceRep {
        InterfaceRep {
            name,
            v_net,
            v_ip,
            status: InterfaceStatus::Up, //Status always starts as Up
            neighbors,
            sender,
        }
    }
    pub fn command(&self, cmd: InterCmd) -> result::Result<(), SendError<InterCmd>> {
        //Sends the input command to the interface
        self.sender.send(cmd)
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
    Send(Packet, Ipv4Addr),                 //Send this packet - when a packet is being forwarded
    ToggleStatus,                           //Make status down if up or up if down
}

//Used to store the data an interface needs to build a packet and send it
#[derive(Debug, Clone)]
pub struct PacketBasis {
    pub dst_ip: Ipv4Addr,
    pub prot_num: u8,
    pub msg: Vec<u8>,
}

// TODO: GET HANDY WITH HANDLERS
// type Handler(&Node, Packet) -> ()
// HandlerTable: HashMap<IpNumber, Handler>
// pub fn register_recv_handler(&mut self, type: IpNumber, function: Handler) -> {
//     self.handlers.insert(type, function);
// }
// pub fn handle_rip(&self, node: &Node, packet: Packet) -> () {
//    
// }

