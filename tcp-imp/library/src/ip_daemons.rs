use crate::prelude::*;
use crate::rip_trait::RipDaemon;
use crate::rip_utils::*;
use crate::utils::*;
use crate::vnode_traits::*;
//use crate::tcp_utils::*;

#[derive(Debug)]
pub struct HostIpDaemon {
    interface_reps: Arc<RwLock<InterfaceTable>>, //Maps an interface's name to its associated InterfaceRep
    interface_recvers: InterfaceRecvers,
    forwarding_table: Arc<RwLock<ForwardingTable>>,
    backend_sender: Sender<Packet>,
}

impl VnodeIpDaemon for HostIpDaemon {
    fn interface_reps(&self) -> RwLockReadGuard<'_, InterfaceTable> {
        self.interface_reps.read().unwrap()
    }
    fn interface_recvers(&self) -> &InterfaceRecvers {
        &self.interface_recvers
    }
    fn forwarding_table(&self) -> RwLockReadGuard<'_, ForwardingTable> {
        self.forwarding_table.read().unwrap()
    }
    fn forwarding_table_mut(&self) -> RwLockWriteGuard<'_, ForwardingTable> {
        self.forwarding_table.write().unwrap()
    }
    fn backend_sender(&self) -> &Sender<Packet> {
        &self.backend_sender
    }
    // TODO: SHOULD TAKE OTHER PROTOCOLS
    fn local_protocols(&self, protocol: IpNumber, pack: Packet) {
        match protocol {
            IpNumber(6) => {
                self.backend_sender.send(pack).expect("Channel fuckery");
            }
            _ => panic!("Unsupported protocol received"),
        }
    }
}

impl HostIpDaemon {
    pub fn new(
        interface_reps: Arc<RwLock<InterfaceTable>>,
        interface_recvers: InterfaceRecvers,
        forwarding_table: Arc<RwLock<ForwardingTable>>,
        backend_sender: Sender<Packet>,
        //sockman_sender: Sender<SockMand>
    ) -> HostIpDaemon {
        HostIpDaemon {
            interface_reps,
            interface_recvers,
            forwarding_table,
            backend_sender,
        }
    }
    /// Runs the node and spawns interfaces
    pub fn run(self, backend_recver: Receiver<PacketBasis>) {
        //ONGOING TASKS
        //Define mutex to protect self - although each tokio "thread" runs asynchronously instead of concurrently, mutexes are still needed (despite what I originally thought)
        let listen_mutex = Arc::new(Mutex::new(self));
        let backend_mutex = Arc::clone(&listen_mutex);
        //Listen for commands coming over the interface and commands
        thread::spawn(move || RouterIpDaemon::backend_listen(backend_mutex, backend_recver));
        RouterIpDaemon::interface_listen(listen_mutex);
    }
}

#[derive(Debug)]
pub struct RouterIpDaemon {
    interface_reps: Arc<RwLock<InterfaceTable>>, //Maps an interface's name to its associated InterfaceRep
    interface_recvers: InterfaceRecvers,
    forwarding_table: Arc<RwLock<ForwardingTable>>,
    rip_neighbors: RipNeighbors, // Stores route information learned about from neighbors
    backend_sender: Sender<Packet>,
}

impl VnodeIpDaemon for RouterIpDaemon {
    fn interface_reps(&self) -> RwLockReadGuard<'_, InterfaceTable> {
        self.interface_reps.read().unwrap()
    }
    fn interface_recvers(&self) -> &InterfaceRecvers {
        &self.interface_recvers
    }
    fn forwarding_table(&self) -> RwLockReadGuard<'_, ForwardingTable> {
        self.forwarding_table.read().unwrap()
    }
    fn forwarding_table_mut(&self) -> RwLockWriteGuard<'_, ForwardingTable> {
        self.forwarding_table.write().unwrap()
    }
    fn backend_sender(&self) -> &Sender<Packet> {
        &self.backend_sender
    }
    /// Take in a packet destined for the current node and display information from it
    fn local_protocols(&self, protocol: IpNumber, pack: Packet) {
        match protocol {
            etherparse::IpNumber(200) => {
                // Message received is a RIP packet
                self.process_rip_packet(pack);
            }
            _ => panic!("Unsupported protocol received"),
        }
    }
}

impl RipDaemon for RouterIpDaemon {
    fn rip_neighbors(&self) -> &RipNeighbors {
        &self.rip_neighbors
    }
}

impl RouterIpDaemon {
    pub fn new(
        interface_reps: Arc<RwLock<InterfaceTable>>,
        interface_recvers: InterfaceRecvers,
        forwarding_table: Arc<RwLock<ForwardingTable>>,
        rip_neighbors: RipNeighbors,
        backend_sender: Sender<Packet>,
    ) -> RouterIpDaemon {
        RouterIpDaemon {
            interface_reps,
            interface_recvers,
            forwarding_table,
            rip_neighbors,
            backend_sender,
        }
    }
    /// Runs the node and spawns interfaces
    pub fn run(self, backend_recver: Receiver<PacketBasis>) {
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
    fn process_rip_packet(&self, pack: Packet) {
        let src_ip: Ipv4Addr = pack.header.source.into();
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
}
