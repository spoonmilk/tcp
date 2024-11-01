use crate::prelude::*;
use crate::rip_utils::*;
use crate::utils::*;
use crate::vnode_traits::*;
use crate::rip_trait::RipDaemon;

// Add creation time to table, subtract from current time, if greater than 12 secs refresh
// Only pertain to things with next hops

type RouterHandler = fn(&RouterIpDaemon, Packet) -> Result<()>;
type HandlerTable = HashMap<IpNumber, RouterHandler>;

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
    fn process_packet(&self, pack: Packet) -> () {
        let protocol = pack.header.protocol;
        let handler_table = self.handler_table();
        if handler_table.contains_key(&protocol) {
            let handler = handler_table.get(&protocol).unwrap();
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
    fn handler_table(&self) -> RwLockReadGuard<HandlerTable> { self.handler_table.read().unwrap() }
    fn handler_table_mut(&self) -> RwLockWriteGuard<HandlerTable> { self.handler_table.write().unwrap() }
    fn register_recv_handler(&mut self, protocol: IpNumber, function: RouterHandler) -> () {
        self.handler_table_mut().insert(protocol, function);
    }
}

