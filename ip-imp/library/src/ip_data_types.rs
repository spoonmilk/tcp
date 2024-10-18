use crate::prelude::*;
use crate::rip_utils::*;
use crate::utils::*;
use std::mem;

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
    rip_neighbors: HashMap<Ipv4Net, Route>,
    // RIP neighbors vec
    // Timeout table(?)
}

impl Node {
    pub fn new(
        n_type: NodeType,
        interfaces: Vec<Interface>,
        interface_reps: HashMap<String, InterfaceRep>,
        forwarding_table: HashMap<Ipv4Net, Route>,
        rip_neighbors: HashMap<Ipv4Net, Route>,
    ) -> Node {
        Node {
            n_type,
            interfaces,
            interface_reps,
            forwarding_table,
            rip_neighbors,
        }
    }

    /// Runs the node and spawns interfaces
    pub fn run(mut self, recv_rchan: Receiver<CmdType>) -> () {
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
        thread::spawn(move || Node::repl_listen(self_mutex1, recv_rchan));
        //Listen for messages from interfaces and handle them
        Node::interface_listen(self_mutex2);
    }
    /// Listen for REPL commands to the node
    fn repl_listen(slf_mutex: Arc<Mutex<Node>>, recv_rchan: Receiver<CmdType>) -> () {
        loop {
            let chan_res = recv_rchan.recv();
            let mut slf = slf_mutex.lock().unwrap();
            match chan_res {
                Ok(CmdType::Li) => slf.li(),
                Ok(CmdType::Ln) => slf.ln(),
                Ok(CmdType::Lr) => slf.lr(),
                Ok(CmdType::Up(inter)) => slf.up(inter),
                Ok(CmdType::Down(inter)) => slf.down(inter),
                Ok(CmdType::Send(addr, msg)) => slf.send(addr, msg),
                Err(e) => panic!("Error receiving from repl channel: {e:?}"),
            }
        }
    }
    /// Listen for messages on node interfaces
    fn interface_listen(slf_mutex: Arc<Mutex<Node>>) -> () {
        loop {
            //println!("Waiting for messages from interfaces");
            let mut packets = Vec::new();
            let mut slf = slf_mutex.lock().unwrap();
            for inter_rep in slf.interface_reps.values_mut() {
                let chan = &mut inter_rep.chan;
                match chan.recv.try_recv() {
                    Ok(pack) => packets.push(pack), //Can't call slf.forward_packet(pack) directly here for ownership reasons
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        panic!("Channel disconnected for some reason")
                    }
                }
            }
            //println!("Received {} packets from interfaces", packets.len());
            for pack in packets {
                println!("Forwarding packet");
                slf.forward_packet(pack).expect("Error forwarding packet");
            }
        }
    }
    /// List interfaces of a node
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
    /// List neighbors of a node
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
    /// List routes from a node
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
    /// Enable an interface
    fn up(&mut self, inter: String) -> () {
        let inter_rep = self.interface_reps.get_mut(&inter).unwrap();
        match inter_rep.status {
            InterfaceStatus::Up => {} //Don't do anything if already up
            InterfaceStatus::Down => {
                inter_rep
                    .command(InterCmd::ToggleStatus)
                    .expect("Error connecting to interface");
                inter_rep.status = InterfaceStatus::Up;
            }
        }
    }
    /// Disable an interface
    fn down(&mut self, inter: String) -> () {
        let inter_rep = self.interface_reps.get_mut(&inter).unwrap();
        match inter_rep.status {
            InterfaceStatus::Up => {
                inter_rep
                    .command(InterCmd::ToggleStatus)
                    .expect("Error connecting to interface");
                inter_rep.status = InterfaceStatus::Down;
            }
            InterfaceStatus::Down => {} //Don't do anything if already down
        }
    }
    /// Send a packet generated by the node
    fn send(&mut self, addr: String, msg: String) -> () {
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
            .expect("Error sending connecting to interface or sending packet"); //COULD BE MORE ROBUST
    }
    /// Forward a packet to the node or to the next hop
    fn forward_packet(&mut self, pack: Packet) -> std::result::Result<(), SendError<InterCmd>> {
        //Made it  cause it'll give some efficiency gains with sending through the channel (I think)
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
        inter_rep.command(InterCmd::Send(pack, next_hop))
    }
    /// Find the interface to forward a packet to
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
    /// Check the validity of a packet
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
    /// Update packet checksum and info
    fn update_pack(pack: Packet) -> Packet {
        // Get header
        let mut pack_head: Ipv4Header = pack.header;
        // Decrement ttl
        let ttl = pack_head.time_to_live;
        if ttl != 0 {
            pack_head.time_to_live = ttl - 1;
        } else {
            eprintln!("Encountered a packet with invalid TTL ; something is wrong");
        }
        pack_head.header_checksum = pack_head.calc_header_checksum();

        // Rebuild packet
        let updated_pack: Packet = Packet {
            header: pack_head,
            data: pack.data,
        };
        return updated_pack;
    }
    /// Longest prefix matching for packet forwarding
    fn longest_prefix(masks: Vec<&Ipv4Net>, addr: &Ipv4Addr) -> Result<Ipv4Net> {
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
    /// Take in a packet destined for the current node and display information from it
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
    fn send_rip(&mut self, fwd_table: &mut HashMap<Ipv4Net, Route>, dst: Ipv4Addr, command: u16) -> () {
        match command {
            1 => { // Send a routing request
                let rip_req_msg: RipMsg = RipMsg::new(1, 0, Vec::new());
                let ser_req_rip: Vec<u8> = serialize_rip(rip_req_msg);
                self.send(dst.to_string(), String::from_utf8(ser_req_rip).unwrap());
            }
            2 => { // Send a routing response
                let rip_resp_msg: RipMsg = table_to_rip(fwd_table, 1);
                let ser_resp_rip: Vec<u8> = serialize_rip(rip_resp_msg);
                self.send(dst.to_string(), String::from_utf8(ser_resp_rip).unwrap());
            }
            _ => panic!("Invalid RIP command type!"),
        }
    }
    fn rip_respond(&mut self) -> () {
        // TODO: Split horizon/poison reverse adding

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
