use std::thread;
use crate::prelude::*;
use crate::utils::*;
use std::mem;

pub static CHANNEL_CAPACITY: usize = 32;

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
    pub async fn run(&mut self, mut recv_rchan: Receiver<CmdType>) -> () {
        //STARTUP TASKS
        //Spawn all interfaces - interfaces is DEPLETED after this and unusable
        let interfaces = mem::take(&mut self.interfaces);
        for interface in interfaces {
            thread::spawn(move || interface.run());
        }
        
        //ONGOING TASKS
        //Listen for REPL prompts from REPL thread and handle them
        let mut repl_listen = tokio::spawn(async move {
            loop {
                match recv_rchan.recv().await {
                    Some(CmdType::Li) => self.li(),
                    Some(CmdType::Ln) => self.ln(),
                    Some(CmdType::Lr) => self.lr(),
                    Some(CmdType::Up(inter)) => self.up(inter),
                    Some(CmdType::Down(inter)) => self.down(inter),
                    Some(CmdType::Send(addr, msg)) => self.send(addr, msg),
                    None => panic!("Channel to REPL disconnected :(")
                }
            }
        });
        //Listen for messages from interfaces and handle them
        let mut interface_listen = tokio::spawn(async {
            loop {
                for inter_rep in self.interface_reps.values() {
                    let chan = inter_rep.chan;
                    match chan.recv.try_recv() {
                        Ok(pack) => self.forward_packet(pack, None),
                        Err(TryRecvError::Empty) => {},
                        Err(TryRecvError::Disconnected) => panic!("Channel disconnected for some reason")
                    }
                }
                tokio::task::yield_now().await; //Make sure listening for messages from interfaces doesn't hof all the time
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
    fn up(&self, inter: String) -> () {}
    fn down(&self, inter: String) -> () {}
    fn send(&self, addr: String, msg: String) -> () {}
    fn forward_packet(&self, pack: Packet, ip: Option<Ipv4Addr>) -> Result<()> {
        //Run it through check_packet to see if it should be dropped
        if !Node::packet_valid(pack) {return Ok(())};
        //Extract Destination Ip address
        let dst_ip = match ip {
            Some(addr) => addr,
            None => pack.dst_addr
        };
        //Run it through longest prefix
        let netmask = Node::longest_prefix(self.forwarding_table.keys().collect(), dst_ip);
        //See what the value tied to that prefix is
        let route = self.forwarding_table.get(&netmask).unwrap();
        //If it's an Ip address, repeat with that IP address, an interface, forward via channel to that interface, if it is a ToSelf, handle internally
        match route {
            //DO STUFF
        }
        //Return
    }
    fn packet_valid(pack: Packet) -> bool {}
    fn longest_prefix(masks: Vec<&Ipv4Net>, addr: Ipv4Addr) -> Ipv4Net {}
    fn process_packet(pack: Packet) -> () {}
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