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
    #[tokio::main]
    pub async fn run(&mut self, mut recv_rchan: Receiver<CmdType>) -> () {
        //Spawn all interfaces
        let interfaces = mem::take(&mut self.interfaces);
        for interface in interfaces {
            thread::spawn(move || interface.run());
        }

        //Listen for REPL prompts from REPL thread and handle them
        let mut repl_listen = tokio::spawn(async move {
            loop {
                match recv_rchan.recv().await {
                    Some(CmdType::Li) => {},
                    Some(CmdType::Ln) => {},
                    Some(CmdType::Lr) => {},
                    Some(CmdType::Up(inter)) => {},
                    Some(CmdType::Down(inter)) => {},
                    Some(CmdType::Send(addr, msg)) => {},
                    None => panic!("Channel to REPL disconnected :(")
                }
            }
        });
        //Listen for messages from interfaces and handle them
        let mut interface_listen = tokio::spawn(async {
            loop {
                println!("Me too!!!");
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
    //fn check_packet(pack: Packet) -> bool {}
    //fn forward_packet(pack: Packet) -> Result<()> {
        //Run it through check_packet to see if it should be dropped
        //Extract Destination Ip address
        //Run it through longest prefix
        //See what the value tied to that prefix is
        //If it's an Ip address, repeat with that IP address, an interface, forward via channel to that interface, if it is a ToSelf, handle internally
        //Return
    //}
    //fn longest_prefix(masks: Vec<Ipv4Net>, addr: Ipv4Addr) -> Ipv4Net {}
    //fn process_packet(pack: Packet) -> () {}
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