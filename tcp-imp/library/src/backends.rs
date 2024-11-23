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

#[derive(Clone)]
pub struct HostBackend {
    interface_reps: Arc<RwLock<InterfaceTable>>,
    forwarding_table: Arc<RwLock<ForwardingTable>>,
    pub socket_table: Arc<RwLock<SocketTable>>, //Just pub for REPL - once IpHandler is made by config this goes away
    pub socket_manager: Arc<Mutex<SocketManager>>, //Just pub for REPL - once IpHandler is made by config this goes away
    local_ip: Ipv4Addr,
    closed_sender: Arc<Sender<SocketId>>,
    ip_sender: Arc<Sender<PacketBasis>>,
    sid_assigner: Arc<SidAssigner>
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
        let (closed_send, closed_recv) = channel::<SocketId>();
        let closed_sender = Arc::new(closed_send);
        let sid_assigner: Arc<SidAssigner> = Arc::new(SidAssigner::new());
        let socket_manager =  SocketManager::new(Arc::clone(&socket_table), Arc::clone(&closed_sender), Arc::clone(&ip_sender), Arc::clone(&sid_assigner));
        let socket_manager = Arc::new(Mutex::new(socket_manager));
        let socket_table_clone = Arc::clone(&socket_table);
        thread::spawn(move || Self::check_closed(socket_table_clone, closed_recv));
        HostBackend { interface_reps, forwarding_table, socket_table, socket_manager, local_ip, closed_sender, ip_sender, sid_assigner }
    }
    pub fn socket_table(&self) -> RwLockReadGuard<SocketTable> { self.socket_table.read().unwrap() }
    fn socket_table_mut(&self) -> RwLockWriteGuard<SocketTable> { self.socket_table.write().unwrap() }
    fn sock_arc(&self, sid: &SocketId) -> Option<Arc<Mutex<ConnectionSocket>>> {
        let s_table = self.socket_table();
        match s_table.get(&sid) {
            Some(SocketEntry::Connection(s_ent)) => Some(Arc::clone(&s_ent.sock)),
            Some(SocketEntry::Listener(_)) => None,
            None => None
        }
    }
    pub fn listen(&self, port: u16) -> SocketId { self.socket_manager.lock().unwrap().listen(port) }
    pub fn accept(&self, port: u16) -> () { self.socket_manager.lock().unwrap().accept(port); }
    pub fn accept1(&self, port: u16) -> Option<SocketId> { 
        let conn_wait = {
            let mut sock_man = self.socket_manager.lock().unwrap();
            sock_man.accept1(port) //Doesn't block 
        };
        if let Some(conn_wait) = conn_wait {
            let sock_arc = conn_wait.recv().expect("Error receiving arc of socket from sender");
            let sid = ConnectionSocket::get_sid(sock_arc);
            Some(sid)
        } else { None }
    }
    pub fn connect(&self, ip_addr: Ipv4Addr, port: u16) -> SocketId { self.init_new_conn(ip_addr, port) }
    fn init_new_conn(&self, dst_vip: Ipv4Addr, dst_port: u16) -> SocketId {
        let conn_src_addr = self.unused_tcp_addr();
        let conn_dst_addr = TcpAddress::new(dst_vip, dst_port);
        let init_state = Arc::new(RwLock::new(TcpState::AwaitingRun));
        // TODO: REfactor after connectionsocket refactoring
        let conn_sock = ConnectionSocket::new(init_state, conn_src_addr.clone(), conn_dst_addr.clone(), Arc::clone(&self.closed_sender), Arc::clone(&self.ip_sender));
        let pending_conn = PendingConn::new(conn_sock);
        let mut socket_table = self.socket_table_mut();
        let sid = self.sid_assigner.assign_sid();
        let sock = pending_conn.start(&mut socket_table, sid); 
        ConnectionSocket::first_syn(sock); //Sends SYN message to start handshaked
        sid
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
    pub fn find_conn_socket(socket_table: RwLockReadGuard<SocketTable>, dst_ip: &Ipv4Addr, port: &u16) -> Option<SocketId> {
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
    pub fn tcp_send(&self, sid: SocketId, data: Vec<u8>) -> Result<u16> {
        let sock = match self.sock_arc(&sid) {
            Some(sock) => sock,
            None => return Err(Error::new(ErrorKind::InvalidInput, "Input socket ID does not match that of any connection sockets"))
        };
        ConnectionSocket::send(sock, data)
    }
    pub fn tcp_recieve(&self, sid: SocketId, bytes: u16) -> Result<Vec<u8>> {
        let sock = match self.sock_arc(&sid) {
            Some(sock) => sock,
            None => return Err(Error::new(ErrorKind::InvalidInput, "Input socket ID does not match that of any connection sockets"))
        };
        ConnectionSocket::receive(sock, bytes)
    }
    pub fn close(&self, sid: SocketId) -> Result<()> {
        let sock_ent = {
            match self.socket_table().get(&sid) {
                Some(sock_ent) => sock_ent.clone(),
                None => return Err(Error::new(ErrorKind::InvalidInput, "Input socket ID does not match that of any sockets"))
            }
        };
        match sock_ent {
            SocketEntry::Connection(ent) => {
                let sock = Arc::clone(&ent.sock);
                ConnectionSocket::close(sock);
            },
            SocketEntry::Listener(ent) => {
                { //Remove from socket table
                    let mut sock_table = self.socket_table_mut();
                    sock_table.remove(&sid).expect("Somehow sid not in socket table but it was 2 milliseconds ago");
                }
                //Remove from listener table
                let mut socket_manager = self.socket_manager.lock().unwrap();
                socket_manager.listener_close(ent);
            }
        }
        Ok(())
    }
    fn check_closed(socket_table: Arc<RwLock<SocketTable>>, closed_recv: Receiver<SocketId>) {
        loop {
            let sid = closed_recv.recv().unwrap();
            let mut sock_table = socket_table.write().unwrap();
            sock_table.remove(&sid).expect("Socket Id to remove doesn't exist within the table... Hmmmmm...");
        }
    }
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