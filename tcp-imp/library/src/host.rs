use crate::prelude::*;
use crate::utils::*;
use crate::vnode_traits::*;

// Add creation time to table, subtract from current time, if greater than 12 secs refresh
// Only pertain to things with next hops
type HostHandler = fn(&HostIpDaemon, Packet) -> Result<()>;
type HandlerTable = HashMap<IpNumber, HostHandler>;

#[derive(Debug)]
pub struct HostIpDaemon {
    interface_reps: Arc<RwLock<InterfaceTable>>, //Maps an interface's name to its associated InterfaceRep
    interface_recvers: InterfaceRecvers,
    forwarding_table: Arc<RwLock<ForwardingTable>>,
    handler_table: Arc<RwLock<HandlerTable>>,
    // Socket table!
}

impl VnodeIpDaemon for HostIpDaemon {
    fn interface_reps(&self) ->  RwLockReadGuard<InterfaceTable> { self.interface_reps.read().unwrap() }
    fn interface_recvers(&self) -> &InterfaceRecvers { &self.interface_recvers }
    fn forwarding_table(&self) -> RwLockReadGuard<ForwardingTable> { self.forwarding_table.read().unwrap() }
    fn forwarding_table_mut(&self) -> RwLockWriteGuard<ForwardingTable> { self.forwarding_table.write().unwrap() }
    fn process_packet(&self, pack: Packet) -> () {
        let protocol = pack.header.protocol;
        let handler_table = self.handler_table();
        if handler_table.contains_key(&protocol) {
            let handler = handler_table.get(&protocol).unwrap();
            handler(&self, pack);
        }
    }
}

impl HostIpDaemon {
    pub fn new(
        interface_reps: InterfaceTable,
        interface_recvers: InterfaceRecvers,
        forwarding_table: ForwardingTable,
        handler_table: HandlerTable,
    ) -> HostIpDaemon {
        HostIpDaemon {
            interface_reps: Arc::new(RwLock::new(interface_reps)),
            interface_recvers,
            forwarding_table: Arc::new(RwLock::new(forwarding_table)),
            handler_table: Arc::new(RwLock::new(handler_table)),
        }
    }
    /// Runs the node and spawns interfaces
    pub fn run(self, backend_recver: Receiver<PacketBasis>) -> () {
        //STARTUP TASKS

        //ONGOING TASKS
        //Define mutex to protect self - although each tokio "thread" runs asynchronously instead of concurrently, mutexes are still needed (despite what I originally thought)
        let listen_mutex = Arc::new(Mutex::new(self));
        let backend_mutex = Arc::clone(&listen_mutex);
        //Listen for commands coming over the interface and commands 
        thread::spawn(move || HostIpDaemon::backend_listen(backend_mutex, backend_recver));
        HostIpDaemon::interface_listen(listen_mutex);
    }
    fn handler_table(&self) -> RwLockReadGuard<HandlerTable> { self.handler_table.read().unwrap() }
    fn handler_table_mut(&self) -> RwLockWriteGuard<HandlerTable> { self.handler_table.write().unwrap() }
    fn register_recv_handler(&mut self, protocol: IpNumber, function: HostHandler) -> () {
        self.handler_table_mut().insert(protocol, function);
    }
    fn tcp_send(&self, pb: PacketBasis) -> () { println!("This SHOULD send a TCP packet right about now.") } // TODO:
}


