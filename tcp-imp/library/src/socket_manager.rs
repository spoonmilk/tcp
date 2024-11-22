use crate::prelude::*;
use crate::tcp_utils::*;
use crate::utils::*;
use crate::sockman_utils::*;
use crate::conn_socket::ConnectionSocket;

pub struct SocketManager {
    socket_table: Arc<RwLock<SocketTable>>,
    listener_table: ListenerTable,
    closed_sender: Arc<Sender<SocketId>>,
    ip_sender: Arc<Sender<PacketBasis>>, 
    sid_assigner: Arc<SidAssigner>
}

impl SocketManager {
    /// Create a new SocketManager, listener table initially empty
    pub fn new(socket_table: Arc<RwLock<SocketTable>>, closed_sender: Arc<Sender<SocketId>>, ip_sender: Arc<Sender<PacketBasis>>, sid_assigner: Arc<SidAssigner>) -> SocketManager {
        SocketManager { socket_table, listener_table: HashMap::new(), closed_sender, ip_sender, sid_assigner }
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
        //Figure out if a socket exists to handle the packet, pass it to the socket if it does, or tell the proper listener to create a connection socket for it (or drop it if no listeners for it)
        self.listener_recv(port, head, tcp_pack);
    }
    /// Adds a listener to the listener table and socket table
    pub fn listen(&mut self, port: u16) -> SocketId {
        {
            self.listener_table.insert(port, ListenerEntry::new());
        }
        {
            let mut socket_table = self.socket_table.write().unwrap();
            let sid = socket_table.len() as u16; //THIS TOO
            let sock_listen_ent = ListenEntry::new(port);
            let listen_ent = SocketEntry::Listener(sock_listen_ent);
            socket_table.insert(sid, listen_ent);
            return sid
        }
    }
    /// Opens a listener on <port> to accepting new connections
    pub fn accept(&mut self, port: u16) -> () {
        let listener_table = &mut self.listener_table;
        match listener_table.get_mut(&port) {
            Some(listener) => {
                listener.accepting = true;
                let mut sock_table = self.socket_table.write().unwrap();
                let sid = self.sid_assigner.assign_sid();
                listener.pending_connections.drain(..).for_each(|pd_conn| { pd_conn.start(&mut sock_table, sid); });
            },
            None => return // Listener was closed in the before this function got c
        };
    }
    pub fn accept1(&mut self, port: u16) -> Option<Receiver<Arc<Mutex<ConnectionSocket>>>> {
        let listener_table = &mut self.listener_table;
        match listener_table.get_mut(&port) {
            Some(listener) => {
                let (sock_send, sock_recv) = channel::<Arc<Mutex<ConnectionSocket>>>();
                if listener.pending_connections.len() > 0 { //Just take the first pending connection
                    let pd_conn = listener.pending_connections.remove(0);
                    let mut sock_table = self.socket_table.write().unwrap();
                    let sid = self.sid_assigner.assign_sid();
                    let sock = pd_conn.start(&mut sock_table, sid);
                    sock_send.send(sock).expect("Error sending arc of sock to receiver");
                } else { //No pending connections, so just say we are open to them for now
                    listener.accepting = true;
                    listener.sock_send = Some(sock_send);
                }
                Some(sock_recv)
            },
            None => None // Listener was closed in the before this function got c
        }
    }
    /// Upon receiving a 'listen' command from the REPL, creates a new listener socket and adds it to the listener table
    /// Also adds a new entry to the socket table to represent the listener socket
    /// Listener sockets will default to a non-accepting state until accept is called.
    /// 
    /// NOTE: The socket table is locked during this operation
    fn listener_recv(&mut self, port: u16, ip_head: Ipv4Header, tcp_pack: TcpPacket) -> () {
        //Check that the packet is a SYN packet and drop if it isn't
        if !has_only_flags(&tcp_pack.header, SYN) { return println!("Listener socket received non SYN packet for some reason") }
        //Find data about appropriate listener socket in the listener table
        let listener = self.listener_table.get_mut(&port).expect("Herm, listener table not synced up with socket table");
        //Construct connection socket and pending connection for incoming client
        let src_addr = TcpAddress::new(Ipv4Addr::from(ip_head.destination), tcp_pack.header.destination_port);
        let dst_addr = TcpAddress::new(Ipv4Addr::from(ip_head.source), tcp_pack.header.source_port);
        let state = Arc::new(RwLock::new(TcpState::Initialized)); //Always start in Initialize state when spawned by listener socket 
        let ip_send = self.ip_sender.clone();
        let closed_send = self.closed_sender.clone();
        let conn_sock = ConnectionSocket::new(state, src_addr.clone(), dst_addr.clone(), closed_send, ip_send);
        let pending_conn = PendingConn::new(conn_sock);
        //Decide whether to immediately start connection or stash it for later depending on whether the listener is accepting
        match listener.accepting {
            true => {
                let sock = {
                    let mut sock_table = self.socket_table.write().unwrap();
                    let sid = self.sid_assigner.assign_sid();
                    pending_conn.start(&mut sock_table, sid) 
                };
                let sock_clone = Arc::clone(&sock); //potentially needed later
                ConnectionSocket::handle_packet(sock, tcp_pack, ip_head); //Sends SYN + ACK message
                if let Some(sock_send) = &listener.sock_send { //Accept1 was called earlier
                    sock_send.send(sock_clone).expect("Error sending arc of socket to receiver");
                    listener.accepting = false;
                    listener.sock_send = None;
                }
            }
            false => listener.pending_connections.push(pending_conn)
        }
    }
    pub fn listener_close(&mut self, listen_ent: &ListenEntry) -> () {
        let listen_port = listen_ent.port;

        let mut sock_table = self.socket_table.write().unwrap();
        match sock_table.get(&listen_port) {
            Some(entry) => {
                match entry {
                    SocketEntry::Listener(_) => {
                        sock_table.remove(&listen_port);
                    }
                    _ => {}
                }
            },
            None => {}
        }
        self.listener_table.remove(&listen_port); 
    }
}