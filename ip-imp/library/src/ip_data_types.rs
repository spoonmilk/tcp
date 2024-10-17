use crate::prelude::*;
use crate::utils::*;
use crate::rip_utils::*;
use std::mem;
use std::thread;

pub static CHANNEL_CAPACITY: usize = 32;

#[derive(Debug, Clone)]
pub enum NodeType {
    Router,
    Host,
}

#[derive(Debug)]
pub struct Node {
    pub n_type: NodeType,
    interfaces: Vec<Interface>, //Is depleted upon startup when all interface threads are spawned - use interface_reps to find information about each interface
    interface_reps: HashMap<String, InterfaceRep>, //Maps an interface's name to its associated InterfaceRep
    forwarding_table: HashMap<Ipv4Net, Route>,
    rip_neighbors: HashMap<Ipv4Net, RipRoute>,
    // RIP neighbors vec
    // Timeout table(?)
}

impl Node {
    pub fn new(
        n_type: NodeType,
        interfaces: Vec<Interface>,
        interface_reps: HashMap<String, InterfaceRep>,
        forwarding_table: HashMap<Ipv4Net, Route>,
        rip_neighbors: HashMap<Ipv4Net, RipRoute>,
    ) -> Node {
        Node {
            n_type,
            interfaces,
            interface_reps,
            forwarding_table,
            rip_neighbors
        }
    }

    /// Runs the node and spawns interfaces
    #[tokio::main]
    pub async fn run(mut self, mut recv_rchan: Receiver<CmdType>) -> () {
        //STARTUP TASKS
        //Spawn all interfaces - interfaces is DEPLETED after this and unusable
        let interfaces = mem::take(&mut self.interfaces);
        println!("Spawning {} interfaces", interfaces.len());
        for interface in interfaces {
            thread::spawn(move || interface.run());
        }
        println!("Interfaces spawned");

        //ONGOING TASKS
        //Define mutex to protect self - although each tokio "thread" runs asynchronously instead of concurrently, mutexes are still needed (despite what I originally thought)
        let self_mutex1 = Arc::new(Mutex::new(self));
        let self_mutex2 = Arc::clone(&self_mutex1);
        println!("Mutexes spawned");

        //Listen for REPL prompts from REPL thread and handle them
        let mut repl_listen = tokio::spawn(async move {
            println!("Listening for REPL");
            loop {
                println!("Waiting for REPL command");
                let chan_res = recv_rchan.recv().await;
                println!("REPL command received");
                let mut slf = self_mutex1.lock().await;
                match chan_res {
                    Some(CmdType::Li) => {
                        slf.li();
                        println!("Send li command")
                    }
                    Some(CmdType::Ln) => {
                        slf.ln();
                        println!("Send ln command")
                    }
                    Some(CmdType::Lr) => {
                        slf.lr();
                        println!("Send lr command")
                    }
                    Some(CmdType::Up(inter)) => {
                        println!("Sending up command");
                        slf.up(inter).await;
                        println!("Up command sent")
                    }
                    Some(CmdType::Down(inter)) => {
                        println!("Sending down command");
                        slf.down(inter).await;
                        println!("Down command sent")
                    }
                    Some(CmdType::Send(addr, msg)) => {
                        println!("Sending message");
                        slf.send(addr, msg).await;
                        println!("Message sent")
                    }
                    None => panic!("Channel to REPL disconnected :("),
                }
                println!("REPL command handled");
                tokio::task::yield_now().await
            }
        });
        //Listen for messages from interfaces and handle them
        let mut interface_listen = tokio::spawn(async move {
            println!("Listening for interfaces");
            loop {
                println!("Waiting for messages from interfaces");
                let mut packets = Vec::new();
                let mut slf = self_mutex2.lock().await;
                for inter_rep in slf.interface_reps.values_mut() {
                    let chan = &mut inter_rep.chan;
                    match chan.recv.try_recv() {
                        Ok(pack) => packets.push(pack), //Can't call slf.forward_packet(pack) directly here for ownership reasons
                        Err(TryRecvError::Empty) => { }
                        Err(TryRecvError::Disconnected) => {
                            panic!("Channel disconnected for some reason")
                        }
                    }
                }
                println!("Received {} packets from interfaces", packets.len());
                for pack in packets {
                    println!("Forwarding packet");
                    slf.forward_packet(pack)
                        .await
                        .expect("Error forwarding packet");
                }
                tokio::task::yield_now().await
            }
        });
        //Select whether to listen for stuff from the REPL or to listen for interface messages
        loop {
            tokio::select! {
                _ = &mut repl_listen => { 
                    println!("Repl listener task completed");
                },
                _ = &mut interface_listen => { 
                    println!("Interface listener task completed");
                }
            }
        }
    }
    fn li(&self) -> () {
        println!("Name\tAddr/Prefix\tState");
        for inter_rep in self.interface_reps.values() {
            let status = match inter_rep.status {
                InterfaceStatus::Up => "up",
                InterfaceStatus::Down => "down",
            };
            println!(
                "{}\t{}/{}\t{}",
                inter_rep.name,
                inter_rep.v_net.addr(),
                inter_rep.v_net.prefix_len(),
                status
            )
        }
    }
    fn ln(&self) -> () {
        println!("Iface\tVIP\t\tUDPAddr");
        for inter_rep in self.interface_reps.values() {
            for neighbor in &inter_rep.neighbors {
                println!(
                    "{}\t{}\t127.0.0.1:{}",
                    inter_rep.name, neighbor.0, neighbor.1
                );
            }
        }
    }
    fn lr(&self) -> () {
        println!("T\tPrefix\t\tNext hop\tCost");
        for (v_net, route) in &self.forwarding_table {
            let cost = match &route.cost {
                Some(num) => num.to_string(),
                None => String::from("-"),
            };
            let next_hop = match &route.next_hop {
                ForwardingOption::Ip(ip) => ip.to_string(),
                ForwardingOption::Inter(inter) => "LOCAL:".to_string() + inter,
                ForwardingOption::ToSelf => continue, //Skip because don't print routes to self
            };
            let r_type = match route.rtype {
                RouteType::Rip => "R",
                RouteType::Local => "L",
                RouteType::Static => "S",
                RouteType::ToSelf => continue, //Should never get here
            };
            println!(
                "{}\t{}/{}\t{}\t{}",
                r_type,
                v_net.addr(),
                v_net.prefix_len(),
                next_hop,
                cost
            )
        }
    }
    async fn up(&mut self, inter: String) -> () {
        let inter_rep = self.interface_reps.get_mut(&inter).unwrap();
        match inter_rep.status {
            InterfaceStatus::Up => {} //Don't do anything if already up
            InterfaceStatus::Down => {
                inter_rep
                    .command(InterCmd::ToggleStatus)
                    .await
                    .expect("Error connecting to interface");
                inter_rep.status = InterfaceStatus::Up;
            }
        }
    }
    async fn down(&mut self, inter: String) -> () {
        let inter_rep = self.interface_reps.get_mut(&inter).unwrap();
        match inter_rep.status {
            InterfaceStatus::Up => {
                inter_rep
                    .command(InterCmd::ToggleStatus)
                    .await
                    .expect("Error connecting to interface");
                inter_rep.status = InterfaceStatus::Down;
            }
            InterfaceStatus::Down => {} //Don't do anything if already down
        }
    }
    async fn send(&mut self, addr: String, msg: String) -> () {
        let ip_addr = addr.as_str().parse().expect("Invalid ip address"); //FIX THIS LATER
        let pb = PacketBasis {
            dst_ip: ip_addr,
            msg,
        };
        let (inter_rep, next_hop) = match self.proper_interface(&ip_addr) {
            Some((name, next_hop)) => (
                self.interface_reps.get_mut(&name.clone()).unwrap(),
                next_hop,
            ),
            None => panic!("Packet sent to self"), //FIX THIS LATER
        };
        inter_rep
            .command(InterCmd::BuildSend(pb, next_hop))
            .await
            .expect("Error sending connecting to interface or sending packet"); //COULD BE MORE ROBUST
    }
    async fn forward_packet(
        &mut self,
        pack: Packet,
    ) -> std::result::Result<(), SendError<InterCmd>> {
        //Made it async cause it'll give some efficiency gains with sending through the channel (I think)
        //Run it through check_packet to see if it should be dropped
        if !Node::packet_valid(pack.clone()) {
            return Ok(());
        };
        let pack = Node::update_pack(pack);
        let pack_header = pack.clone().header;
        //Get the proper interface's name
        let (inter_rep_name, next_hop) =
            match self.proper_interface(&Ipv4Addr::from(pack_header.destination)) {
                Some((name, next_hop)) => (name, next_hop),
                None => {
                    self.process_packet(pack);
                    return Ok(());
                }
            };
        //Find the proper interface and hand the packet off to it
        let inter_rep_name = inter_rep_name.clone(); //Why? To get around stinkin Rust borrow checker. Get rid of this line (and the borrow on the next) to see why. Ugh
        let inter_rep = self.interface_reps.get_mut(&inter_rep_name).unwrap();
        inter_rep.command(InterCmd::Send(pack, next_hop)).await
    }
    fn proper_interface(&self, dst_addr: &Ipv4Addr) -> Option<(&String, Ipv4Addr)> {
        let mut dst_ip = dst_addr;
        loop {
            //Loop until bottom out at a route that sends to an interface
            //Run it through longest prefix
            let netmask = Node::longest_prefix(self.forwarding_table.keys().collect(), dst_ip)
                .expect("Couldn't find matching prefix for {netmask:?}");
            //See what the value tied to that prefix is
            let route = self.forwarding_table.get(&netmask).unwrap();
            //If it's an Ip address, repeat with that IP address, an interface, forward via channel to that interface, if it is a ToSelf, handle internally
            dst_ip = match &route.next_hop {
                ForwardingOption::Inter(name) => break Some((name, dst_ip.clone())),
                ForwardingOption::Ip(ip) => ip,
                ForwardingOption::ToSelf => break None,
            };
        }
    }
    fn packet_valid(pack: Packet) -> bool {
        // Get header
        let pack_head: Ipv4Header = pack.header;

        // Obtain ttl, check if not zero
        let ttl = pack_head.time_to_live;
        if ttl == 0 {
            return false;
        }

        // Obtain checksum, check if correct calculation
        let checksum = pack_head.header_checksum;
        let checksum_correct = pack_head.calc_header_checksum();
        return checksum == checksum_correct;
    }
    fn update_pack(pack: Packet) -> Packet {
        // Get header
        let mut pack_head: Ipv4Header = pack.header;
        // Decrement ttl
        let ttl = pack_head.time_to_live;
        if ttl != 0 {
            pack_head.time_to_live = ttl - 1;
        } else {
            eprintln!("What the fuck");
        }
        pack_head.header_checksum = pack_head.calc_header_checksum();

        // Rebuild packet
        let updated_pack: Packet = Packet {
            header: pack_head,
            data: pack.data,
        };
        return updated_pack;
    }

    /// There's a way to do this with a trie, but I'm unsure if I... want to.
    fn longest_prefix(masks: Vec<&Ipv4Net>, addr: &Ipv4Addr) -> Result<Ipv4Net> {
        // Put the thing in

        // // Create a vector of Ipv4Net prefixes/addrs
        // let pref_vec: Vec<Ipv4Addr> = Vec::new();
        // let trie_vec: Vec<TrieNode> = Vec::new();

        // For now, linear search
        let mut trie_node = TrieNode::new();
        for mask in masks {
            trie_node.insert(mask.network());
        }
        match trie_node.search(addr) {
            Ok(search_res) => {
                let address: Ipv4Addr = search_res.parse().expect("fuck");
                return Ok(Ipv4Net::from(address));
            }
            Err(e) => Err(e),
        }
    }
    fn process_packet(&self, pack: Packet) -> () {
        let src = String::from_utf8(Vec::from(pack.header.source)).unwrap();
        let dst = String::from_utf8(Vec::from(pack.header.destination)).unwrap();
        let ttl = pack.header.time_to_live;
        let msg = String::from_utf8(pack.data).unwrap();
        let retstr = format!(
            "Received tst packet: Src: {}, Dst: {}, TTL: {}, {}",
            src, dst, ttl, msg
        );
        println!("{}", retstr);
        // Logic for editing fwd table
    }
}

#[derive(Debug)]
pub enum CmdType {
    Li,
    Ln,
    Lr,
    Up(String),
    Down(String),
    Send(String, String),
}
