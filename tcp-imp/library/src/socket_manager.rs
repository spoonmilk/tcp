use rand::Rng;
use crate::prelude::*;
use crate::tcp_utils::*;
use crate::utils::*;
use crate::conn_socket::ConnectionSocket;

pub struct SocketManager {
    // TODO: If time allows, get rid of reference to backend and replace with functions to communicate between 
    // IP daemon and socket manager
    local_ip: Ipv4Addr,
    // TODO: Make u16, not usize, as no sockets should have id > 65535
    socket_table: Arc<RwLock<SocketTable>>,
    listener_table: ListenerTable,
    //backend_sender: Sender<String>,
    ip_sender: Arc<Sender<PacketBasis>>
}

impl SocketManager {
    /// Create a new SocketManager, listener table initially empty
    pub fn new(local_ip: Ipv4Addr, socket_table: Arc<RwLock<SocketTable>>, ip_sender: Arc<Sender<PacketBasis>>) -> SocketManager {
        SocketManager { local_ip, socket_table, listener_table: HashMap::new(), ip_sender }
    } 
    /// Initialize connection socket for incoming packet and either add it to pending connections for listener or add it to socket table
    pub fn handle_incoming(&mut self, pack: Packet, port: u16) -> () {
        let head = pack.header;
        let body = pack.data;

        if head.protocol != IpNumber::from(6) {
            eprintln!("TCP handler received non-TCP packet, dropping.");
            return;
        }

        let tcp_pack = deserialize_tcp(body).expect("Malformed TCP packet");
        let socket_table = self.socket_table.read().unwrap();
        //Figure out if a socket exists to handle the packet, pass it to the socket if it does, or tell the proper listener to create a connection socket for it (or drop it if no listeners for it)

        match socket_table.get(&port) {
            Some(SocketEntry::Connection(_ent)) => {
                eprintln!("Ouch! Not supposed to be here.");
            }
            Some(SocketEntry::Listener(ent)) => {
                let l_port = ent.port.clone();
                drop(socket_table);
                self.listener_recv(l_port, head, tcp_pack);
            }
            None => {
                panic!("Internal logic issue - check proper_socket");
            }
        }
    }
    /// Adds a listener to the listener table and socket table
    pub fn listen(&mut self, port: u16) -> () {
        {
            let mut socket_table = self.socket_table.write().unwrap();
            let sid = socket_table.len() as u16 + 1;
            let sock_listen_ent = ListenEntry::new(port);
            let listen_ent = SocketEntry::Listener(sock_listen_ent);
            socket_table.insert(sid, listen_ent);
        }

        {
            self.listener_table.insert(port, ListenerEntry::new());
        }
    }
    /// Opens a listener on <port> to accepting new connections
    pub fn accept(&mut self, port: u16) -> () {
        let listener_table = &mut self.listener_table;
        match listener_table.get_mut(&port) {
            Some(listener) => {
                listener.accepting = true;
            },
            None => return // Listener was closed in the before this function got c
        };
    }
    ///Finds the proper socket for a TcpPacket given an associated IP header
    ///NOTE: Takes the socket table as an input to avoid nasty synchronization bugs - this might actually be unnecessary now that I think about it lol
    fn proper_socket(ip_head: &Ipv4Header, tcp_pack: &TcpPacket, socket_table: &RwLockReadGuard<SocketTable>) -> Option<SocketId> {
        //let socket_table = self.socket_table.read().unwrap();
        //Extract necessary data
        let src_ip = Ipv4Addr::from(ip_head.source);
        let dst_ip = Ipv4Addr::from(ip_head.destination);
        let src_port = &tcp_pack.header.source_port;
        let dst_port = &tcp_pack.header.destination_port;
        //Loop through and find the proper socket ID
        let mut listener_id = None;
        for (sock_id, sock_entry) in &**socket_table {
            match sock_entry {
                SocketEntry::Connection(ent) => {
                    if (ent.dst_addr.ip == src_ip) && (ent.src_addr.ip == dst_ip) && (ent.dst_addr.port == *src_port) && (ent.src_addr.port == *dst_port) {
                        return Some(sock_id.clone());
                    }
                },
                SocketEntry::Listener(ent) => {
                    if (ent.port == *dst_port) && is_syn(&tcp_pack.header) { listener_id = Some(sock_id.clone()) }
                }
            }
        }
        listener_id
    }
    /// Upon receiving a 'listen' command from the REPL, creates a new listener socket and adds it to the listener table
    /// Also adds a new entry to the socket table to represent the listener socket
    /// Listener sockets will default to a non-accepting state until accept is called.
    /// 
    /// NOTE: The socket table is locked during this operation
    fn listener_recv(&mut self, port: u16, ip_head: Ipv4Header, tcp_pack: TcpPacket) -> () {
        //Find data about appropriate listener socket in the listener table
        let listener = self.listener_table.get_mut(&port).expect("Herm, listener table not synced up with socket table");
        //Construct pending connection for incoming client
        let src_addr = TcpAddress::new(Ipv4Addr::from(ip_head.destination), tcp_pack.header.destination_port);
        let dst_addr = TcpAddress::new(Ipv4Addr::from(ip_head.source), tcp_pack.header.source_port);
        let state = Arc::new(RwLock::new(TcpState::SynRecvd)); //Always start in syn recved state when spawned by listener socket
        
        // Clone arc of ip_sender, create new connection socket
        let ip_send = self.ip_sender.clone();
        let conn_sock = ConnectionSocket::new(state, src_addr.clone(), dst_addr.clone(), ip_send);
        let (sock_sender, sockman_recver) = channel::<SocketCmd>();

        let mut sock_table = self.socket_table.write().unwrap();
        let pending_conn = PendingConn { sock: conn_sock, sockman_recver, sock_sender };
        //Decide whether to immediately start connection or stash it for later depending on whether the listener is accepting
        match listener.accepting {
            true => pending_conn.start(&mut sock_table), //Technically, if we wanted to be fully faithful to a true socket API, we would set accepting back to false here, but this doesn't actually need to happen, so...
            false => listener.pending_connections.push(pending_conn)
        }
    }
    /// Initializes a new connection socket and adds it to the socket table
    /// Default socket state will be AwaitingRun
    /// 
    /// NOTE: The socket table is locked during this operation
    fn init_new_conn(&self, dst_vip: Ipv4Addr, dst_port: u16) -> () {
        let conn_src_addr = self.unused_tcp_addr();
        let conn_dst_addr = TcpAddress::new(dst_vip, dst_port);
        let init_state = Arc::new(RwLock::new(TcpState::AwaitingRun));
        // Clone arc of ip_sender, create new connection socket
        let ip_send = self.ip_sender.clone();
        let conn_sock = ConnectionSocket::new(init_state, conn_src_addr.clone(), conn_dst_addr.clone(), ip_send);
        let (conn_send, conn_recv) = channel::<SocketCmd>();
        let new_conn = PendingConn::new(conn_sock, conn_recv, conn_send);

        let mut sock_table = self.socket_table.write().unwrap();
        new_conn.start(&mut sock_table);
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
}

//Socket manager utils
//Had to move these out of tcp_utils because conn_socket depends on tcp_utils but these data structures depend on conn_socket...

pub type SocketId = u16;
pub type SocketTable = HashMap<SocketId, SocketEntry>;
pub type ListenerTable = HashMap<u16, ListenerEntry>;

pub enum SocketEntry {
    Connection(ConnectionEntry),
    Listener(ListenEntry)
}

#[derive(Debug)]
pub struct ConnectionEntry {
    pub src_addr: TcpAddress,
    pub dst_addr: TcpAddress, 
    pub state: Arc<RwLock<TcpState>>,
    //pub sender: Sender<SocketCmd>
    pub sock: Arc<Mutex<ConnectionSocket>>
}

pub struct ListenEntry { 
    pub port: u16,
    pub state: Arc<RwLock<TcpState>>
}
impl ListenEntry {
    pub fn new(port: u16) -> ListenEntry {
        ListenEntry { port, state: Arc::new(RwLock::new(TcpState::Listening)) }
    }
}

pub struct ListenerEntry {
    pub accepting: bool,
    pub pending_connections: Vec<PendingConn>
}
impl ListenerEntry {
    pub fn new() -> ListenerEntry {
        ListenerEntry { accepting: false, pending_connections: Vec::new() }
    }
}

pub struct PendingConn {
    pub sock: ConnectionSocket, //Connection socket that has not been run yet - run it when the associated listener socket starts accepting connections
    pub sockman_recver: Receiver<SocketCmd>, //Receiver for the connection socket to receive from the socket manager
    pub sock_sender: Sender<SocketCmd> //Sender for the socket manager to send to the connection socker - goes in socket table
}

impl PendingConn {
    pub fn new(sock: ConnectionSocket, sockman_recver: Receiver<SocketCmd>, sock_sender: Sender<SocketCmd>) -> PendingConn {
        PendingConn { sock, sockman_recver, sock_sender }
    }
    /// Takes in a pending connection and spawns it as a new thread
    pub fn start(self, socket_table: &mut RwLockWriteGuard<SocketTable>) -> () {
        //Create entry on socket table and add it
        let sid = socket_table.len() as SocketId;
        let src_addr = (&self.sock.src_addr).clone();
        let dst_addr = (&self.sock.dst_addr).clone();
        let state = Arc::clone(&self.sock.state);
        let sock = Arc::new(Mutex::new(self.sock));
        // TODO: refactor after connectionsocket change
        let ent = ConnectionEntry { src_addr, dst_addr, state, sock };
        let ent = SocketEntry::Connection(ent);
        socket_table.insert(sid, ent);
    }
}

// BEWARE ALL YE WHO ENTER: THE LEGACY CODE ZONE
// 
//    fn listen_backend(slf_mutex: Arc<Mutex<Self>>, backend_recver: Receiver<SockMand>) -> () {
//        loop {
//            let incoming_command = backend_recver.recv().unwrap();
//            let mut slf = slf_mutex.lock().unwrap();
//            match incoming_command {
//                SockMand::Listen(port) => {
//                    // Acquire self lock
//                    let mut slf = slf_mutex.lock().unwrap();
//                    // Modify socket table to add information about the new listener port, state
//                    {
//                        let mut socket_table = slf.socket_table.write().unwrap();
//                        let sid = socket_table.len();
//                        let sock_listen_ent = ListenEntry { port, state: Arc::new(RwLock::new(TcpState::Listening)) };
//                        let listen_ent = SocketEntry::Listener(sock_listen_ent);
//                        socket_table.insert(sid, listen_ent);
//                    }
//                    // Modify listener -> default state is not accepting any connections
//                    {
//                        let listener_table = &mut slf.listener_table;
//                        listener_table.insert(port, SocketManager::vlisten());
//                    }
//                },
//                SockMand::Accept(port) => {
//                    slf.vaccept(port);
//                }, 
//                SockMand::Connect(vip, port) => {
//                    slf.init_new_conn(vip, port);
//                }
//
//            }
//        }
//    }
// 
//     fn listen_ip(slf_mutex: Arc<Mutex<Self>>, ip_recver: Receiver<Packet>) -> () {
//         loop {
//             //Listen for an incoming IP packet from IP daemon
//             let incoming_packet = ip_recver.recv().expect("Error receiving from IpDaemon");
//             //Lock self and self.socket_table and deserialize the tcp packet
//             let mut slf = slf_mutex.lock().unwrap();
//             let tcp_pack = deserialize_tcp(incoming_packet.data).expect("Malformed TCP packet");
//             let socket_table = slf.socket_table.read().unwrap();
//             //Figure out if a socket exists to handle the packet, pass it to the socket if it does, or tell the proper listener to create a connection socket for it (or drop it if no listeners for it)
//             match SocketManager::proper_socket(&incoming_packet.header, &tcp_pack, &socket_table) {
//                 Some(sock_id) => {
//                     let sock_entry = socket_table.get(&sock_id).expect("Internal logic issue - check proper_socket");
//                     match sock_entry {
//                         SocketEntry::Connection(ent) => ent.sender.send(SocketCmd::Process(tcp_pack)).expect("Error sending tcppacket to connection socket"),
//                         SocketEntry::Listener(ent) => { //Blegh, ownership
//                             let port = ent.port.clone();
//                             drop(socket_table);
//                             slf.listener_recv(port, incoming_packet.header, tcp_pack)
//                         }
//                     }
//                 }
//                 None => {} //Drop the packet - no sockets care about it lol
//             }
//         }
//     }