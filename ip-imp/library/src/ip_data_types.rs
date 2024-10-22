use crate::prelude::*;
use crate::rip_utils::*;
use crate::utils::*;
use std::io::{Error, ErrorKind};
use std::mem;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
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
    rip_neighbors: HashMap<Ipv4Addr, Vec<Route>>, // Stores route information learned about from neighbors
    // RIP neighbors vec
    // Timeout table(?)
}

impl Node {
    pub fn new(
        n_type: NodeType,
        interfaces: Vec<Interface>,
        interface_reps: HashMap<String, InterfaceRep>,
        forwarding_table: HashMap<Ipv4Net, Route>,
        rip_neighbors: HashMap<Ipv4Addr, Vec<Route>>,
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
        let my_type = self.n_type.clone();

        //STARTUP TASKS
        //Spawn all interfaces - interfaces is DEPLETED after this and unusable
        let interfaces = mem::take(&mut self.interfaces);
        for interface in interfaces {
            thread::spawn(move || interface.run());
        }

        // Broadcast rip request to all neighbors
        self.request_all();

        //ONGOING TASKS
        //Define mutex to protect self - although each tokio "thread" runs asynchronously instead of concurrently, mutexes are still needed (despite what I originally thought)
        let self_mutex1 = Arc::new(Mutex::new(self));
        let self_mutex2 = Arc::clone(&self_mutex1);
        let self_mutex3 = Arc::clone(&self_mutex2);
        let self_mutex4 = Arc::clone(&self_mutex3);
        //Listen for REPL prompts from REPL thread and handle them
        thread::spawn(move || Node::repl_listen(self_mutex1, recv_rchan));


        if my_type == NodeType::Router {
            thread::spawn(move || Node::rip_go(self_mutex2));
            thread::spawn(move || Node::run_table_check(self_mutex3));
        }
        
        //Listen for messages from interfaces and handle them
        Node::interface_listen(self_mutex4);
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
                Ok(CmdType::Send(addr, msg)) => slf.send(addr, msg, true),
                Err(e) => panic!("Error receiving from repl channel: {e:?}"),
            }
        }
    }
    /// Periodically broadcasts RIP updates
    fn rip_go(slf_mutex: Arc<Mutex<Node>>) {
        loop {
            thread::sleep(Duration::from_secs(5));
            let mut slf = slf_mutex.lock().unwrap();
            slf.rip_broadcast();
        }
    }
    /// Periodically checks the entries of the forwarding table
    fn run_table_check(slf_mutex: Arc<Mutex<Node>>) {
        loop {
            // // Get relevant checking variables then drop for wait
            // let slf= slf_mutex.lock().unwrap();
            // let old_table = slf.forwarding_table.clone();
            // std::mem::drop(slf); // Drop for wait, let's not block the others!
            // Wait for time to pass
            thread::sleep(Duration::from_secs(12));
            // Now check the similarity of the two
            let slf = slf_mutex.lock().unwrap();
            println!("Checking forwarding table of len {}" , slf.forwarding_table.len());
            // for (v_net, route) in &old_table {
            //     // If a route has been changed
            //     let new_route = slf.forwarding_table.get(v_net).unwrap();
            //     let new_route_rtype = route.rtype.clone();
            //     let new_route_dest = route.next_hop.clone();
            //     let net = v_net.clone();

            //     if new_route != route {
            //         // Insert, update, remove
            //         let replace = Route::new(new_route_rtype, Some(16), new_route_dest);
            //         slf.forwarding_table.insert(net, replace);
            //         slf.rip_broadcast();
            //         slf.forwarding_table.remove(&net);
            //     }
            // }
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
    fn send(&mut self, addr: String, msg: String, msg_type: bool) -> () {
        let ip_addr = addr.as_str().parse().expect("Invalid ip address"); //FIX THIS LATER
        let pb = PacketBasis {
            dst_ip: ip_addr,
            msg,
        };
        let (inter_rep, next_hop) = match self.proper_interface(&ip_addr) {
            Ok(Some((name, next_hop))) => (
                self.interface_reps.get_mut(&name.clone()).unwrap(),
                next_hop,
            ),
            Ok(None) => panic!("Packet sent to self"), //FIX THIS LATER
            Err(e) => return println!("\nForwarding table entry for address not found: {e:?}")
        };
        if msg_type { // For Test Packets
            inter_rep
            .command(InterCmd::BuildSend(pb, next_hop, true))
            .expect("Error sending connecting to interface or sending packet"); //COULD BE MORE ROBUST
        } else { // For RIP
            inter_rep
            .command(InterCmd::BuildSend(pb, next_hop, false))
            .expect("Error sending connecting to interface or sending packet"); //COULD BE MORE ROBUST
        }
        
    }
    /// Forward a packet to the node or to the next hop
    fn forward_packet(&mut self, pack: Packet) -> Result<()> {//std::result::Result<(), SendError<InterCmd>> {
        //Made it  cause it'll give some efficiency gains with sending through the channel (I think)
        //Run it through check_packet to see if it should be dropped
        if !Node::packet_valid(pack.clone()) {
            return Ok(());
        };
        let pack = Node::update_pack(pack);
        let pack_header = pack.clone().header;
        //Get the proper interface's name
        let (inter_rep_name, next_hop) =
            match self.proper_interface(&Ipv4Addr::from(pack_header.destination))? {
                Some((name, next_hop)) => (name, next_hop),
                None => {
                    self.process_packet(pack);
                    return Ok(());
                }
            };
        //Find the proper interface and hand the packet off to it
        let inter_rep_name = inter_rep_name.clone(); //Why? To get around stinkin Rust borrow checker. Get rid of this line (and the borrow on the next) to see why. Ugh
        let inter_rep = self.interface_reps.get_mut(&inter_rep_name).unwrap();
        match inter_rep.command(InterCmd::Send(pack, next_hop)) {
            Ok(()) => Ok(()),
            Err(_) => Err(Error::new(ErrorKind::Other, "Send Error"))
        }
    }
    /// Find the interface to forward a packet to
    fn proper_interface(&self, dst_addr: &Ipv4Addr) -> Result<Option<(&String, Ipv4Addr)>> {
        let mut dst_ip = dst_addr;
        loop {
            //Loop until bottom out at a route that sends to an interface
            //Run it through longest prefix
            let netmask = Node::longest_prefix(self.forwarding_table.keys().collect(), dst_ip)?;
            //See what the value tied to that prefix is
            let route = self.forwarding_table.get(&netmask).unwrap();
            //If it's an Ip address, repeat with that IP address, an interface, forward via channel to that interface, if it is a ToSelf, handle internally
            dst_ip = match &route.next_hop {
                ForwardingOption::Inter(name) => break Ok(Some((name, dst_ip.clone()))),
                ForwardingOption::Ip(ip) => ip,
                ForwardingOption::ToSelf => break Ok(None),
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
    fn longest_prefix(prefixes: Vec<&Ipv4Net>, addr: &Ipv4Addr) -> Result<Ipv4Net> {
        if prefixes.len() < 1 { return Err(Error::new(ErrorKind::Other, "No prefixes to search through")) }
        let mut current_longest: Option<Ipv4Net> = None;
        for prefix in prefixes {
            if prefix.contains(addr) {
                match current_longest {
                    Some(curr_prefix) if curr_prefix.prefix_len() < prefix.prefix_len() => current_longest = Some(prefix.clone()),
                    None => current_longest = Some(prefix.clone()),
                    _ => {}
                }
            }
        }
        match current_longest {
            Some(prefix) => Ok(prefix),
            None => Err(Error::new(ErrorKind::Other, "No matching prefix found"))
        }
    }
    /*fn longest_prefix(masks: Vec<&Ipv4Net>, addr: &Ipv4Addr) -> Result<Ipv4Net> {
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
    }*/
    /// Take in a packet destined for the current node and display information from it
    fn process_packet(&mut self, pack: Packet) -> () {
        let src = Node::string_ip(pack.header.source);
        let dst = Node::string_ip(pack.header.destination);
        let src_ip: Ipv4Addr = pack.header.source.into();
        let ttl = pack.header.time_to_live;
        
        match pack.header.protocol {
            IpNumber(0) => { // Message received is a test packet
                let msg = String::from_utf8(pack.data).unwrap();
                let retstr = format!("Received tst packet: Src: {}, Dst: {}, TTL: {}, {}",src, dst, ttl, msg);
                println!("{}", retstr);
            }
            IpNumber(200) => { // Message received is a RIP packet
                let rip_msg_vec: Vec<u8> = pack.data.to_vec();
                let rip_msg = deserialize_rip(&rip_msg_vec);
                println!("Received RIP packet: {:?}", rip_msg);
                // Edit the forwarding table
                match rip_msg.command {
                    1 => {  // Received a routing request
                        self.send_rip(src_ip, 2); // Send a routing response
                    }
                    2 => { // Received a routing response
                        self.update_fwd_table(rip_msg); // Update the forwarding table according to the response
                        self.rip_broadcast(); // Broadcast the new routing table
                    }
                    _ => panic!("Unsupported RIP command received"),
                }
            }
            _ => panic!("Unsupported protocol received"),
        }
        // Logic for editing fwd table
    }
    fn string_ip(raw_ip: [u8; 4]) -> String {
        Vec::from(raw_ip).iter().map(|num| num.to_string()).collect::<Vec<String>>().join(".")
    }
    fn send_rip(&mut self, dst: Ipv4Addr, command: u16) -> () {
        match command {
            1 => { // Send a routing request
                let rip_req_msg: RipMsg = RipMsg::new(1, 0, Vec::new());
                let ser_req_rip: Vec<u8> = serialize_rip(rip_req_msg);
                self.send(dst.to_string(), String::from_utf8(ser_req_rip).unwrap(), false);
            }
            2 => { // Send a routing response
                let rip_resp_msg: RipMsg = table_to_rip(&mut self.forwarding_table, &self.rip_neighbors, 1, dst);
                let ser_resp_rip: Vec<u8> = serialize_rip(rip_resp_msg);
                self.send(dst.to_string(), String::from_utf8(ser_resp_rip).unwrap(), false);
            }
            _ => panic!("Invalid RIP command type!"),
        }
    }
    fn rip_broadcast(&mut self) -> () { // For periodic and triggered updates
        let keys: Vec<Ipv4Addr> = self.rip_neighbors.keys().cloned().collect(); // So tired of this ownership bullshit
        for addr in keys {
            self.send_rip(addr, 2);
        }
    }
    fn rip_request(&mut self, dst: Ipv4Addr) -> () {
        self.send_rip(dst, 1);
    }
    fn request_all(&mut self) -> () {
        let keys: Vec<Ipv4Addr> = self.rip_neighbors.keys().cloned().collect(); // So tired of this ownership bullshit
        for addr in keys {
            self.rip_request(addr);
        }
    }
    /// Updates a node's RIP table according to a RIP message
    fn update_fwd_table(&mut self, rip_msg: RipMsg) {
        for route in rip_msg.routes {
            route_update(route, &mut self.forwarding_table);
        }
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
