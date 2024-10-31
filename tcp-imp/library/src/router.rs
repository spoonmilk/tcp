use crate::prelude::*;
use crate::rip_utils::*;
use crate::utils::*;
use crate::vnode_traits::*;

// Add creation time to table, subtract from current time, if greater than 12 secs refresh
// Only pertain to things with next hops

#[derive(Debug)]
pub struct RouterIPDaemon {
    interface_reps: Arc<RwLock<InterfaceTable>>, //Maps an interface's name to its associated InterfaceRep
    interface_recvers: InterfaceRecvers,
    forwarding_table: Arc<RwLock<ForwardingTable>>,
    rip_neighbors: RipNeighbors, // Stores route information learned about from neighbors
}

impl VnodeIPDaemon for RouterIPDaemon {
    fn interface_reps(&self) ->  RwLockReadGuard<InterfaceTable> { self.interface_reps.read().unwrap() }
    fn interface_recvers(&self) -> &InterfaceRecvers { &self.interface_recvers }
    fn forwarding_table(&self) -> RwLockReadGuard<ForwardingTable> { self.forwarding_table.read().unwrap() }
    fn forwarding_table_mut(&self) -> RwLockWriteGuard<ForwardingTable> { self.forwarding_table.write().unwrap() }
    /// Take in a packet destined for the current node and display information from it
    fn process_packet(&mut self, pack: Packet) -> () {
        let src = Router::string_ip(pack.header.source);
        let dst = Router::string_ip(pack.header.destination);
        let src_ip: Ipv4Addr = pack.header.source.into();
        let ttl = pack.header.time_to_live;

        match pack.header.protocol {
            IpNumber(0) => {
                // Message received is a test packet
                let msg = String::from_utf8(pack.data).unwrap();
                let retstr = format!(
                    "Received tst packet: Src: {}, Dst: {}, TTL: {}, {}",
                    src,
                    dst,
                    ttl,
                    msg
                );
                println!("{}", retstr);
            }
            IpNumber(200) => {
                // Message received is a RIP packet
                let rip_msg_vec: Vec<u8> = pack.data;
                let mut rip_msg = deserialize_rip(rip_msg_vec);
                // Edit the forwarding table
                match rip_msg.command {
                    1 => {
                        // Received a routing request
                        self.rip_respond(src_ip, None); // Send a routing response
                    }
                    2 => {
                        // Received a routing response
                        let changed_routes = self.update_fwd_table(&mut rip_msg, src_ip); // Update the forwarding table according to the response
                        self.triggered_update(changed_routes);
                    }
                    _ => panic!("Unsupported RIP command received"),
                }
            }
            _ => panic!("Unsupported protocol received"),
        }
    }
}

impl Router {
    pub fn new(
        interface_reps: InterfaceTable,
        forwarding_table: ForwardingTable,
        rip_neighbors: RipNeighbors
    ) -> Router {
        Router {
            interface_reps,
            forwarding_table,
            rip_neighbors,
        }
    }

    /// Runs the node and spawns interfaces
    pub fn run(mut self, recv_rchan: Receiver<CmdType>) -> () {
        //STARTUP TASKS
        //Request RIP routes from neighboring routers
        thread::sleep(Duration::from_millis(100)); //Make sure all routers have been initialized before requesting
        self.request_all();

        //ONGOING TASKS
        //Define mutex to protect self - although each tokio "thread" runs asynchronously instead of concurrently, mutexes are still needed (despite what I originally thought)
        let listen_mutex = Arc::new(Mutex::new(self));
        let repl_mutex = Arc::clone(&listen_mutex);
        //Listen for REPL prompts from REPL thread and handle them
        let rip_periodic = Arc::clone(&listen_mutex);
        let timeout_check = Arc::clone(&listen_mutex);
        thread::spawn(move || Router::rip_go(rip_periodic));
        thread::spawn(move || Router::run_table_check(timeout_check));
        thread::spawn(move || Router::repl_listen(repl_mutex, recv_rchan));
        //Listen for messages from interfaces and handle them
        Router::interface_listen(listen_mutex);
    }

    /// Listen for REPL commands to the node
    fn repl_listen(slf_mutex: Arc<Mutex<Router>>, recv_rchan: Receiver<CmdType>) -> () {
        loop {
            let chan_res = recv_rchan.recv();
            let mut slf = slf_mutex.lock().unwrap();
            match chan_res {
                Ok(CmdType::Li) => slf.li(),
                Ok(CmdType::Ln) => slf.ln(),
                Ok(CmdType::Lr) => slf.lr(),
                Ok(CmdType::Up(inter)) => slf.up(inter),
                Ok(CmdType::Down(inter)) => slf.down(inter),
                Ok(CmdType::Send(addr, msg)) => slf.send(addr, msg.into(), true),
                Err(e) => panic!("Error receiving from repl channel: {e:?}"),
            }
        }
    }
    /// Periodically broadcasts RIP updates
    fn rip_go(slf_mutex: Arc<Mutex<Router>>) {
        loop {
            thread::sleep(Duration::from_secs(5));
            let mut slf = slf_mutex.lock().unwrap();
            slf.rip_broadcast();
        }
    }
    /// Periodically checks the entries of the forwarding table
    fn run_table_check(slf_mutex: Arc<Mutex<Router>>) {
        loop {
            thread::sleep(Duration::from_millis(12000));
            let mut slf = slf_mutex.lock().unwrap();
            let mut to_remove = Vec::new();
            //Thought it'd be easier just to loop through the forwarding table itself so that updating/deleting the route wouldn't be painful
            for (prefix, route) in &mut slf.forwarding_table {
                match route.rtype {
                    RouteType::Rip if route.creation_time.elapsed().as_millis() >= 12000 => {
                        route.cost = Some(16);
                        to_remove.push(prefix.clone()); //Clone used to avoid stinky borrowing issues
                    }
                    _ => {}
                }
            }
            if !to_remove.is_empty() {
                slf.rip_broadcast();
                to_remove.iter().for_each(|prf| {
                    slf.forwarding_table.remove(prf).expect("Something fishy");
                });
            }
        }
    }
    /// Listen for messages on node interfaces
    fn interface_listen(slf_mutex: Arc<Mutex<Router>>) -> () {
        loop {
            let mut packets = Vec::new();
            let mut slf = slf_mutex.lock().unwrap();
            for inter_rep in slf.interface_reps.values_mut() {
                let chan = &mut inter_rep.chan;
                match chan.recv.try_recv() {
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
    fn string_ip(raw_ip: [u8; 4]) -> String {
        Vec::from(raw_ip)
            .iter()
            .map(|num| num.to_string())
            .collect::<Vec<String>>()
            .join(".")
    }
    fn rip_respond(&mut self, dst: Ipv4Addr, nets: Option<&Vec<Ipv4Net>>) -> () {
        // Send a routing response
        // println!("Sending RIP response to neighbor {}", dst);
        let rip_resp_msg: RipMsg = self.table_to_rip(nets);
        let ser_resp_rip: Vec<u8> = serialize_rip(rip_resp_msg.clone());
        self.send(dst.to_string(), ser_resp_rip, false);
    }
    fn rip_broadcast(&mut self) -> () {
        // For periodic and triggered updates
        let keys: Vec<Ipv4Addr> = self.rip_neighbors.keys().cloned().collect(); // So tired of this ownership bullshit
        for addr in keys {
            self.rip_respond(addr, None);
        }
    }
    fn triggered_update(&mut self, changed_routes: Option<Vec<Ipv4Net>>) -> () {
        match changed_routes {
            Some(changed_routes) => {
                let keys: Vec<Ipv4Addr> = self.rip_neighbors.keys().cloned().collect(); // So tired of this ownership bullshit
                for addr in keys {
                    self.rip_respond(addr, Some(&changed_routes));
                }
            }
            None => {}
        }
    }
    fn rip_request(&mut self, dst: Ipv4Addr) -> () {
        let rip_req_msg: RipMsg = RipMsg::new(1, 0, Vec::new());
        let ser_req_rip: Vec<u8> = serialize_rip(rip_req_msg);
        self.send(dst.to_string(), ser_req_rip, false);
    }
    fn request_all(&mut self) -> () {
        let keys: Vec<Ipv4Addr> = self.rip_neighbors.keys().cloned().collect(); // So tired of this ownership bullshit
        for addr in keys {
            self.rip_request(addr);
        }
    }
    /// Updates a node's RIP table according to a RIP message - returns None if nothing gets changed
    fn update_fwd_table(&mut self, rip_msg: &mut RipMsg, next_hop: Ipv4Addr) -> Option<Vec<Ipv4Net>> {
        let mut updated = Vec::new();
        for route in &mut rip_msg.routes {
            match route_update(route, &mut self.forwarding_table, &next_hop) {
                Some(net) => updated.push(net),
                None => {}
            }
        }
        if !updated.is_empty() { Some(updated) }
        else { None }
    }
    pub fn table_to_rip(&mut self, nets: Option<&Vec<Ipv4Net>>) -> RipMsg {
        let mut rip_routes: Vec<RipRoute> = Vec::new();
        let table = match nets {
            Some(nets) => {
                let mut ftable_subset = HashMap::new();
                nets.iter().for_each(|net| { ftable_subset.insert(net, self.forwarding_table.get(net).expect("Internal Failure: net should def be in the fwding table")); });
                ftable_subset
            }
            None => self.forwarding_table.iter().map(|(key, val)| (key, val)).collect() //Weird ownership wizardry
        };
        for (mask, route) in table {
            match route.rtype {
                RouteType::ToSelf | RouteType::Static => continue,
                _ => {
                    let rip_route = RipRoute::new(
                        route.cost.clone().unwrap(),
                        mask.clone().addr().into(),
                        mask.clone().netmask().into()
                    );
                    rip_routes.push(rip_route);
                }
            }
        }
        RipMsg::new(2, rip_routes.len() as u16, rip_routes.to_vec())
    }
}
