use crate::prelude::*;
use crate::rip_utils::*;
use crate::utils::*;
use crate::vnode_traits::*;
use crate::rip_trait::RipDaemon;

// Add creation time to table, subtract from current time, if greater than 12 secs refresh
// Only pertain to things with next hops

#[derive(Debug)]
pub struct RouterIpDaemon {
    interface_reps: Arc<RwLock<InterfaceTable>>, //Maps an interface's name to its associated InterfaceRep
    interface_recvers: InterfaceRecvers,
    forwarding_table: Arc<RwLock<ForwardingTable>>,
    rip_neighbors: RipNeighbors, // Stores route information learned about from neighbors
    handler_table: Arc<RwLock<HandlerTable>>,
}

impl VnodeIpDaemon for RouterIpDaemon {
    fn interface_reps(&self) ->  RwLockReadGuard<InterfaceTable> { self.interface_reps.read().unwrap() }
    fn interface_recvers(&self) -> &InterfaceRecvers { &self.interface_recvers }
    fn forwarding_table(&self) -> RwLockReadGuard<ForwardingTable> { self.forwarding_table.read().unwrap() }
    fn forwarding_table_mut(&self) -> RwLockWriteGuard<ForwardingTable> { self.forwarding_table.write().unwrap() }
    fn handler_table(&self) -> RwLockReadGuard<HandlerTable> { self.handler_table.read().unwrap() }
    fn handler_table_mut(&self) -> RwLockWriteGuard<HandlerTable> { self.handler_table.write().unwrap() }
    /// Take in a packet destined for the current node and display information from it
    // fn process_packet(&self, pack: Packet) -> () {
    //     let src = RouterIpDaemon::string_ip(pack.header.source);
    //     let dst = RouterIpDaemon::string_ip(pack.header.destination);
    //     let src_ip: Ipv4Addr = pack.header.source.into();
    //     let ttl = pack.header.time_to_live;

    //     match pack.header.protocol {
    //         IpNumber(0) => {
    //             // Message received is a test packet
    //             let msg = String::from_utf8(pack.data).unwrap();
    //             let retstr = format!(
    //                 "Received tst packet: Src: {}, Dst: {}, TTL: {}, {}",
    //                 src,
    //                 dst,
    //                 ttl,
    //                 msg
    //             );
    //             println!("{}", retstr);
    //         }
    //         IpNumber(200) => {
    //             // Message received is a RIP packet
    //             let rip_msg_vec: Vec<u8> = pack.data;
    //             let mut rip_msg = deserialize_rip(rip_msg_vec);
    //             // Edit the forwarding table
    //             match rip_msg.command {
    //                 1 => {
    //                     // Received a routing request
    //                     self.rip_respond(src_ip, None); // Send a routing response
    //                 }
    //                 2 => {
    //                     // Received a routing response
    //                     let changed_routes = self.update_fwd_table(&mut rip_msg, src_ip); // Update the forwarding table according to the response
    //                     self.triggered_update(changed_routes);
    //                 }
    //                 _ => panic!("Unsupported RIP command received"),
    //             }
    //         }
    //         _ => panic!("Unsupported protocol received"),
    //     }
    // }
    fn process_packet(&self, pack: Packet) -> () {
        let protocol = pack.header.protocol;
        if self.handler_table().read().unwrap().contains_key(&protocol) {
            let handler = self.handler_table().get(&protocol).unwrap();
            handler(&self, pack);
        }
    }
}

impl RipDaemon for RouterIpDaemon {
    fn rip_neighbors(&self) -> &RipNeighbors { &self.rip_neighbors }
}

impl RouterIpDaemon {
    pub fn new(
        interface_reps: InterfaceTable,
        interface_recvers: InterfaceRecvers,
        forwarding_table: ForwardingTable,
        handler_table: HandlerTable,
        rip_neighbors: RipNeighbors
    ) -> RouterIpDaemon {
        RouterIpDaemon {
            interface_reps: Arc::new(RwLock::new(interface_reps)),
            interface_recvers,
            forwarding_table: Arc::new(RwLock::new(forwarding_table)),
            handler_table: Arc::new(RwLock::new(handler_table)),
            rip_neighbors,
        }
    }
    /// Runs the node and spawns interfaces
    pub fn run(self, backend_recver: Receiver<PacketBasis>) -> () {
        //STARTUP TASKS
        //Request RIP routes from neighboring RouterIpDaemons
        thread::sleep(Duration::from_millis(100)); //Make sure all RouterIpDaemons have been initialized before requesting
        self.request_all();

        //ONGOING TASKS
        //Define mutex to protect self - although each tokio "thread" runs asynchronously instead of concurrently, mutexes are still needed (despite what I originally thought)
        let listen_mutex = Arc::new(Mutex::new(self));
        let backend_mutex = Arc::clone(&listen_mutex);
        let rip_periodic = Arc::clone(&listen_mutex);
        let timeout_check = Arc::clone(&listen_mutex);
        //Send RIP responses periodically and check the table for route timeouts periodically
        thread::spawn(move || RouterIpDaemon::rip_go(rip_periodic));
        thread::spawn(move || RouterIpDaemon::run_table_check(timeout_check));
        //Listen for commands coming over the interface and commands 
        thread::spawn(move || RouterIpDaemon::backend_listen(backend_mutex, backend_recver));
        RouterIpDaemon::interface_listen(listen_mutex);
    }
}
