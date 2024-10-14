use std::thread;
use crate::prelude::*;
use crate::utils::*;
use std::mem;
use std::sync::Arc;
use tokio::sync::Mutex;

pub static CHANNEL_CAPACITY: usize = 32;
static INF: i32 = 16;

#[derive (Debug, Clone)]
pub enum NodeType {
    Router,
    Host,
}

#[derive (Debug)]
pub struct Node {
    pub n_type: NodeType,
    interfaces: Vec<Interface>, //Is depleted upon startup when all interface threads are spawned - use interface_reps to find information about each interface
    interface_reps: HashMap<String, InterfaceRep>, //Maps an interface's name to its associated InterfaceRep
    forwarding_table: HashMap<Ipv4Net, Route>,
}

impl Node {
    pub fn new(
        n_type: NodeType,
        interfaces: Vec<Interface>, 
        interface_reps: HashMap<String, InterfaceRep>, 
        forwarding_table: HashMap<Ipv4Net, Route>,
    ) -> Node {
        Node {
            n_type,
            interfaces,
            interface_reps,
            forwarding_table,
        }
    }
    #[tokio::main]
    pub async fn run(mut self, mut recv_rchan: Receiver<CmdType>) -> () {
        //STARTUP TASKS
        //Spawn all interfaces - interfaces is DEPLETED after this and unusable
        let interfaces = mem::take(&mut self.interfaces);
        for interface in interfaces {
            thread::spawn(move || interface.run());
        }
        
        //ONGOING TASKS
        //Define mutex to protect self - although each tokio "thread" runs asynchronously instead of concurrently, mutexes are still needed (despite what I originally thought)
        let self_mutex1 = Arc::new(Mutex::new(self));
        let self_mutex2 = Arc::clone(&self_mutex1);
        //Listen for REPL prompts from REPL thread and handle them
        let mut repl_listen = tokio::spawn(async move {
            loop {
                let chan_res = recv_rchan.recv().await;
                let mut slf = self_mutex1.lock().await;
                match chan_res {
                    Some(CmdType::Li) => slf.li(),
                    Some(CmdType::Ln) => slf.ln(),
                    Some(CmdType::Lr) => slf.lr(),
                    Some(CmdType::Up(inter)) => slf.up(inter),
                    Some(CmdType::Down(inter)) => slf.down(inter),
                    Some(CmdType::Send(addr, msg)) => slf.send(addr, msg),
                    None => panic!("Channel to REPL disconnected :(")
                }
            }
        });
        //Listen for messages from interfaces and handle them
        let mut interface_listen = tokio::spawn(async move {
            loop {
                let mut packets = Vec::new();
                let mut slf = self_mutex2.lock().await;
                for inter_rep in slf.interface_reps.values_mut() {
                    let chan = &mut inter_rep.chan;
                    match chan.recv.try_recv() {
                        Ok(pack) => packets.push(pack), //Can't call slf.forward_packet(pack) directly here for ownership reasons
                        Err(TryRecvError::Empty) => {},
                        Err(TryRecvError::Disconnected) => panic!("Channel disconnected for some reason")
                    } 
                }
                packets.into_iter().for_each(|pack| slf.forward_packet(pack).expect("Error forwarding packet"));
                tokio::task::yield_now().await; //Make sure listening for messages from interfaces doesn't hog all the time
            }
        });
        //Select whether to listen for stuff from the REPL or to listen for interface messages
        loop {
            tokio::select! {
                _ = &mut repl_listen => {},
                _ = &mut interface_listen => {}
            }
        }
    }
    fn li(&self) -> () {}
    fn ln(&self) -> () {}
    fn lr(&self) -> () {}
    fn up(&mut self, inter: String) -> () {}
    fn down(&mut self, inter: String) -> () {}
    fn send(&mut self, addr: String, msg: String) -> () {
        let pb = PacketBasis {
            dst_ip: addr,
            msg
        };
        //let inter_name = self.proper_interface(Ipv4Addr::from(addr));
    }
    async fn forward_packet(&mut self, pack: Packet) -> Result<()> { //Made it async cause it'll give some efficiency gains with sending through the channel (I think)
        //Run it through check_packet to see if it should be dropped
        if !Node::packet_valid(&pack) {return Ok(())};
        //Get the proper interface's name
        let inter_rep_name = match self.proper_interface(&pack.dst_addr) {
            Some(name) => name,
            None => {
                self.process_packet(pack);
                return Ok(());
            }
        };
        //Find the proper interface and hand the packet off to it
        let inter_rep_name = inter_rep_name.clone(); //Why? To get around stinkin Rust borrow checker. Get rid of this line (and the borrow on the next) to see why. Ugh
        let inter_rep = self.interface_reps.get_mut(&inter_rep_name).unwrap();
        inter_rep.chan.send.send(InterCmd::Send(pack));
        Ok(())
    }
    fn proper_interface(&self, dst_addr: &Ipv4Addr) -> Option<&String> {
        let mut dst_ip = dst_addr;
        loop { //Loop until bottom out at a route that sends to an interface
            //Run it through longest prefix
            let netmask = Node::longest_prefix(self.forwarding_table.keys().collect(), dst_ip);
            //See what the value tied to that prefix is
            let route = self.forwarding_table.get(&netmask).unwrap();
            //If it's an Ip address, repeat with that IP address, an interface, forward via channel to that interface, if it is a ToSelf, handle internally
            dst_ip = match &route.next_hop {
                ForwardingOption::Inter(name) => break Some(name),
                ForwardingOption::Ip(ip) => ip,
                ForwardingOption::ToSelf => break None
            };
        }
    }
    fn packet_valid(pack: &Packet) -> bool {}

    /// There's a way to do this with a trie, but I'm unsure if I... want to. 
    fn longest_prefix(masks: Vec<&Ipv4Net>, addr: &Ipv4Addr) -> Result<String, Ipv4Net> {
        // Put the thing in
    }

    fn process_packet(&self, pack: Packet) -> () {}
    
}

#[derive (Debug)]
pub enum CmdType {
    Li,
    Ln,
    Lr,
    Up(String),
    Down(String),
    Send(String, String)
}