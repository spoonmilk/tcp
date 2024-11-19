use crate::prelude::*;
use crate::tcp_utils::*;
use crate::utils::*;
use crate::sockman_utils::*;
use crate::conn_socket::ConnectionSocket;

pub struct SocketManager {
    socket_table: Arc<RwLock<SocketTable>>,
    listener_table: ListenerTable,
    closed_sender: Arc<Sender<SocketId>>,
    ip_sender: Arc<Sender<PacketBasis>>
}

impl SocketManager {
    /// Create a new SocketManager, listener table initially empty
    pub fn new(socket_table: Arc<RwLock<SocketTable>>, closed_sender: Arc<Sender<SocketId>>, ip_sender: Arc<Sender<PacketBasis>>) -> SocketManager {
        SocketManager { socket_table, listener_table: HashMap::new(), closed_sender, ip_sender }
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
    pub fn listen(&mut self, port: u16) -> () {
        {
            let mut socket_table = self.socket_table.write().unwrap();
            let sid = socket_table.len() as u16;
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
                //TODO: Needs to start all pending connections
            },
            None => return // Listener was closed in the before this function got c
        };
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
                let mut sock_table = self.socket_table.write().unwrap();
                let sock = pending_conn.start(&mut sock_table); //Technically, if we wanted to be fully faithful to a true socket API, we would set accepting back to false here, but this doesn't actually need to happen, so...
                ConnectionSocket::handle_packet(sock, tcp_pack); //Sends SYN + ACK message
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