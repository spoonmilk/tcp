use crate::prelude::*;
use crate::utils::*;
use std::any::Any;
use rand::seq::IteratorRandom;
use rand::thread_rng;

pub trait VnodeBackend {
    //Getters
    fn interface_reps(&self) -> RwLockReadGuard<InterfaceTable>;
    fn interface_reps_mut(&self) -> RwLockWriteGuard<InterfaceTable>;
    fn forwarding_table(&self) -> RwLockReadGuard<ForwardingTable>;
    fn ip_sender(&self) -> &Sender<PacketBasis>;
    /// List interfaces of a node
    fn li(&self) -> () {
        println!("Name\tAddr/Prefix\tState");
        for inter_rep in self.interface_reps().values() {
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
            );
        }
    }
    /// List neighbors of a node
    fn ln(&self) -> () {
        println!("Iface\tVIP\t\tUDPAddr");
        for inter_rep in self.interface_reps().values() {
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
        for (v_net, route) in &*(self.forwarding_table()) {
            let cost = match &route.cost {
                Some(num) => num.to_string(),
                None => String::from("-"),
            };
            let next_hop = match &route.next_hop {
                ForwardingOption::Ip(ip) => ip.to_string(),
                ForwardingOption::Inter(inter) => "LOCAL:".to_string() + inter,
                ForwardingOption::ToSelf => {
                    continue;
                } //Skip because don't print routes to self
            };
            let r_type = match route.rtype {
                RouteType::Rip => "R",
                RouteType::Local => "L",
                RouteType::Static => "S",
                RouteType::ToSelf => {
                    continue;
                } //Should never get here
            };
            println!(
                "{}\t{}/{}\t{}\t{}",
                r_type,
                v_net.addr(),
                v_net.prefix_len(),
                next_hop,
                cost
            );
        }
    }
    /// Enable an interface
    fn up(&self, inter: String) -> () {
        match self.interface_reps_mut().get_mut(&inter) {
            Some(inter_rep) => {
                match inter_rep.status {
                    InterfaceStatus::Down => {
                        inter_rep
                            .command(InterCmd::ToggleStatus)
                            .expect("Error connecting to interface");
                        inter_rep.status = InterfaceStatus::Up;
                    }
                    InterfaceStatus::Up => {} //Don't do anything if already up
                }
            }
            None => {
                println!("Couldn't find interface: {}", inter);
            }
        }
    }
    /// Disable an interface
    fn down(&self, inter: String) -> () {
        match self.interface_reps_mut().get_mut(&inter) {
            Some(inter_rep) => {
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
            None => {
                println!("Couldn't find interface: {}", inter);
            }
        }
    }
    fn raw_send(&self, pb: PacketBasis) -> () {
        if let Err(e) = self.ip_sender().send(pb) {
            panic!("{e:?}")
        }
    }
    fn as_any(&self) -> &dyn Any;
}

pub trait VnodeIpDaemon {
    //Getters
    fn interface_reps(&self) -> RwLockReadGuard<InterfaceTable>; //DON'T need to MUTATE InterfaceTable
    fn interface_recvers(&self) -> &InterfaceRecvers;
    fn forwarding_table(&self) -> RwLockReadGuard<ForwardingTable>;
    fn forwarding_table_mut(&self) -> RwLockWriteGuard<ForwardingTable>;
    fn backend_sender(&self) -> &Sender<Packet>;
    /// Listen for REPL commands to the node
    fn backend_listen<T: VnodeIpDaemon>(
        slf_mutex: Arc<Mutex<T>>,
        backend_recver: Receiver<PacketBasis>,
    ) -> () {
        loop {
            match backend_recver.recv() {
                Ok(pb) => {
                    let slf = slf_mutex.lock().unwrap();
                    let pack = slf.build(pb);
                    slf.send(pack);
                }
                Err(e) => panic!("Error receiving from backend: {e:?}"),
            }
        }
    }
    /// Listen for messages on node interfaces
    fn interface_listen<T: VnodeIpDaemon>(slf_mutex: Arc<Mutex<T>>) -> () {
        loop {
            let mut packets = Vec::new();
            let mut slf = slf_mutex.lock().unwrap();
            for inter_recv in slf.interface_recvers().values() {
                match inter_recv.try_recv() {
                    Ok(pack) => packets.push(pack), //Can't call slf.forward_packet(pack) directly here for ownership reasons
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        panic!("Channel disconnected for some reason");
                    }
                }
            }
            for pack in packets {
                match slf.forward_packet(pack) {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("Error forwarding packet: {e:?}");
                    }
                }
            }
        }
    }
    // TODO: Results on build fail
    fn build(&self, pb: PacketBasis) -> Packet {
        // Match proper interface to find src ip
        let src_ip = match self.proper_interface(&pb.dst_ip) {
            Ok(Some((inter, _))) => self.interface_reps().get(&inter).unwrap().v_net.addr(),
            Ok(None) => self.interface_reps().iter().choose(&mut thread_rng()).expect("No interfaces?").1.v_net.addr(),
            _ => {
                panic!("Failed to build packet ; fuck")
            }
        };
        // TODO: UPDATE FOR NON RIP/TEST PACKAGES
        let mut header = Ipv4Header {
            source: src_ip.octets(),
            destination: pb.dst_ip.octets(),
            time_to_live: 16, // Generally default
            total_len: Ipv4Header::MIN_LEN_U16 + (pb.msg.len() as u16),
            protocol: pb.prot_num.into(),
            ..Default::default()
        };
        header.header_checksum = header.calc_header_checksum();
        return Packet {
            header,
            data: pb.msg,
        };
    }
    // TODO: Integrate multi-protocol sending/building
    /// Send a packet generated by the node
    fn send(&self, pack: Packet) -> () {
        let inter_reps = self.interface_reps();
        let dst_ip = Ipv4Addr::from(pack.header.destination.clone());
        let (inter_rep, next_hop) = match self.proper_interface(&dst_ip) {
            Ok(Some((name, next_hop))) => (inter_reps.get(&name).unwrap(), next_hop),
            Ok(None) => {
                // Sent to self, process
                return self.process_packet(pack);
            }
            Err(e) => {
                // Panicking shouldn't happen on IP level, just drop the packet
                return eprintln!(
                    "Couldn't find a proper interface for packet, dropping. Error: {e:?}"
                );
            }
        };
        //println!("Sending test packet to next hop: {}", next_hop);
        inter_rep
            .command(InterCmd::Send(pack, next_hop))
            .expect("Error sending connecting to interface or sending packet");
    }
    /// Forward a packet to the node or to the next hop
    fn forward_packet(&mut self, pack: Packet) -> Result<()> {
        //std::result::Result<(), SendError<InterCmd>> {
        //Made it  cause it'll give some efficiency gains with sending through the channel (I think)
        //Run it through check_packet to see if it should be dropped
        if !<Self as VnodeIpDaemon>::packet_valid(pack.clone()) {
            return Ok(());
        }
        let pack = <Self as VnodeIpDaemon>::update_pack(pack);
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
        let binding = self.interface_reps();
        let inter_rep = binding.get(&inter_rep_name).unwrap();
        // Using to show route through nodes
        println!("Forwarding packet to interface: {}", inter_rep_name);
        match inter_rep.command(InterCmd::Send(pack, next_hop)) {
            Ok(()) => Ok(()),
            Err(_) => Err(Error::new(ErrorKind::Other, "Send Error")),
        }
    }
    /// Find the interface to forward a packet to
    fn proper_interface(&self, dst_addr: &Ipv4Addr) -> Result<Option<(String, Ipv4Addr)>> {
        let mut dst_ip = dst_addr;
        //See what the value tied to that prefix is
        let table_lock = self.forwarding_table();
        loop {
            //Loop until bottom out at a route that sends to an interface
            //Run it through longest prefix
            let netmask = <Self as VnodeIpDaemon>::longest_prefix(
                self.forwarding_table().keys().collect(),
                dst_ip,
            )?;
            let route = table_lock.get(&netmask).unwrap();
            //If it's an Ip address, repeat with that IP address, an interface, forward via channel to that interface, if it is a ToSelf, handle internally
            dst_ip = match &route.next_hop {
                ForwardingOption::Inter(name) => break Ok(Some((name.clone(), dst_ip.clone()))),
                ForwardingOption::Ip(ip) => ip,
                ForwardingOption::ToSelf => {
                    break Ok(None);
                }
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
        if prefixes.len() < 1 {
            return Err(Error::new(
                ErrorKind::Other,
                "No prefixes to search through",
            ));
        }
        let mut current_longest: Option<Ipv4Net> = None;
        for prefix in prefixes {
            if prefix.contains(addr) {
                match current_longest {
                    Some(curr_prefix) if curr_prefix.prefix_len() < prefix.prefix_len() => {
                        current_longest = Some(prefix.clone());
                    }
                    None => {
                        current_longest = Some(prefix.clone());
                    }
                    _ => {}
                }
            }
        }
        match current_longest {
            Some(prefix) => Ok(prefix),
            None => Err(Error::new(ErrorKind::Other, "No matching prefix found")),
        }
    }
    fn process_test_packet(&self, pack: Packet) -> () {
        /*
        let src = <Self as VnodeIpDaemon>::string_ip(pack.header.source);
        let dst = <Self as VnodeIpDaemon>::string_ip(pack.header.destination);
        let ttl = pack.header.time_to_live;
        // Message received is a test packet
        let msg = String::from_utf8(pack.data).unwrap();
        let retstr = format!(
            "Received tst packet: Src: {}, Dst: {}, TTL: {}, {}",
            src, dst, ttl, msg
        );
        */
        self.backend_sender()
            .send(pack)
            .expect("Could not send to backend"); 
    }
    fn process_packet(&self, pack: Packet) -> () {
        self.shared_protocols(pack.header.protocol, pack);
    }
    fn shared_protocols(&self, protocol: IpNumber, pack: Packet) -> () {
        match protocol {
            etherparse::IpNumber(0) => {
                self.process_test_packet(pack);
            }
            _ => {
                self.local_protocols(protocol, pack);
            }
        }
    }
    /// Individually expressed processing functions for routers, hosts
    fn local_protocols(&self, _protocol: IpNumber, _pack: Packet) -> () {}
}
