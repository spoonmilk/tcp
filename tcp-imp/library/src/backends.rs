use crate::prelude::*;
use crate::utils::*;
use crate::vnode_traits::*;
use crate::sockman_utils::*;
use crate::socket_manager::SocketManager;
use crate::conn_socket::ConnectionSocket;
use crate::tcp_utils::*;

//I'm thinking that initialize() will now return a Backend, so it'll need this
pub enum  Backend {
    Host(HostBackend),
    Router(RouterBackend)
}

pub struct HostBackend {
    interface_reps: Arc<RwLock<InterfaceTable>>,
    forwarding_table: Arc<RwLock<ForwardingTable>>,
    pub socket_table: Arc<RwLock<SocketTable>>, //Just pub for REPL - once IpHandler is made by config this goes away
    pub socket_manager: Arc<Mutex<SocketManager>>, //Just pub for REPL - once IpHandler is made by config this goes away
    local_ip: Ipv4Addr,
    ip_sender: Arc<Sender<PacketBasis>>
}

impl VnodeBackend for HostBackend {
    fn interface_reps(&self) -> RwLockReadGuard<InterfaceTable> { self.interface_reps.read().unwrap() }
    fn interface_reps_mut(&self) -> RwLockWriteGuard<InterfaceTable> { self.interface_reps.write().unwrap() }
    fn forwarding_table(&self) -> RwLockReadGuard<ForwardingTable> { self.forwarding_table.read().unwrap() }
    fn ip_sender(&self) -> &Sender<PacketBasis> { &self.ip_sender }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

impl HostBackend {
    pub fn new(interface_reps: Arc<RwLock<InterfaceTable>>, forwarding_table: Arc<RwLock<ForwardingTable>>, socket_table: Arc<RwLock<SocketTable>>, ip_sender: Sender<PacketBasis>) -> HostBackend {
        let local_ip = interface_reps.read().unwrap().get("if0").expect("Assumed that if0 would exist").v_ip.clone(); //IDEALLY, THIS IS NOT DONE THIS WAY
        let ip_sender = Arc::new(ip_sender);
        let socket_manager =  SocketManager::new(Arc::clone(&socket_table), Arc::clone(&ip_sender));
        let socket_manager = Arc::new(Mutex::new(socket_manager));
        HostBackend { interface_reps, forwarding_table, socket_table, socket_manager, local_ip, ip_sender }
    }
    pub fn socket_table(&self) -> RwLockReadGuard<SocketTable> { self.socket_table.read().unwrap() }
    pub fn socket_table_mut(&self) -> RwLockWriteGuard<SocketTable> { self.socket_table.write().unwrap() }
    pub fn listen(&self, port: u16) -> () { self.socket_manager.lock().unwrap().listen(port); }
    pub fn accept(&self, port: u16) -> () { self.socket_manager.lock().unwrap().accept(port); }
    pub fn connect(&self, ip_addr: Ipv4Addr, port: u16) -> () {
        let socket_table = self.socket_table();
        if let None = Self::find_conn_socket(socket_table, &ip_addr, &port) {
            self.init_new_conn(ip_addr, port);
        }
    }
    fn init_new_conn(&self, dst_vip: Ipv4Addr, dst_port: u16) -> () {
        let conn_src_addr = self.unused_tcp_addr();
        let conn_dst_addr = TcpAddress::new(dst_vip, dst_port);
        let init_state = Arc::new(RwLock::new(TcpState::AwaitingRun));
        // TODO: REfactor after connectionsocket refactoring
        let conn_sock = ConnectionSocket::new(init_state, conn_src_addr.clone(), conn_dst_addr.clone(), Arc::clone(&self.ip_sender), 0);
        let pending_conn = PendingConn::new(conn_sock);
        let mut socket_table = self.socket_table_mut();
        let sock = pending_conn.start(&mut socket_table);
        ConnectionSocket::first_syn(sock); //Sends SYN message to start handshaked
    }
    /// Generates a new unused TCP address on the local IP
    fn unused_tcp_addr(&self) -> TcpAddress {
        // Acquire local ip
        let local_ip = self.local_ip;
        let sock_table = self.socket_table.read().unwrap();
        // Generate random ports until one is unused
        let mut port = rand::thread_rng().gen_range(0..65535);
        while sock_table.contains_key(&port) {
            port = rand::thread_rng().gen_range(0..65535);
        }
        // Return a new TcpAddress with the local IP and a random port
        TcpAddress::new(local_ip, port as u16)
    }
    fn find_conn_socket(socket_table: RwLockReadGuard<SocketTable>, dst_ip: &Ipv4Addr, port: &u16) -> Option<SocketId> {
        for (sid, ent) in &*socket_table {
            match ent {
                SocketEntry::Connection(ent) if (ent.dst_addr.ip == *dst_ip) && (ent.dst_addr.port == *port) => {
                    return Some(sid.clone());
                },
                SocketEntry::Connection(_) => {} //Not a matching socket
                SocketEntry::Listener(_) => {} //Don't care if it's a listener socket 
            }
        }
        None
    }
    //More to come 
    pub fn tcp_send(&self, pb: PacketBasis) -> () {} //COMING SOON
}

pub struct RouterBackend {
    interface_reps: Arc<RwLock<InterfaceTable>>,
    forwarding_table: Arc<RwLock<ForwardingTable>>,
    ip_sender: Sender<PacketBasis>,
    //NO SOCKET TABLE NEEDED
}

impl VnodeBackend for RouterBackend {
    fn interface_reps(&self) -> RwLockReadGuard<InterfaceTable> { self.interface_reps.read().unwrap() }
    fn interface_reps_mut(&self) -> RwLockWriteGuard<InterfaceTable> { self.interface_reps.write().unwrap() }
    fn forwarding_table(&self) -> RwLockReadGuard<ForwardingTable> { self.forwarding_table.read().unwrap() }
    fn ip_sender(&self) -> &Sender<PacketBasis> { &self.ip_sender }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

impl RouterBackend {
    pub fn new(interface_reps: Arc<RwLock<InterfaceTable>>, forwarding_table: Arc<RwLock<ForwardingTable>>, ip_sender: Sender<PacketBasis>) -> RouterBackend {
        RouterBackend { interface_reps, forwarding_table, ip_sender }
    }
}