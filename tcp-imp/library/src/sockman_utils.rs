use crate::prelude::*;
use crate::tcp_utils::*;
use crate::conn_socket::ConnectionSocket;

pub type SocketId = u16;
pub type SocketTable = HashMap<SocketId, SocketEntry>;
pub type ListenerTable = HashMap<u16, ListenerEntry>;

#[derive(Debug)]
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

#[derive(Debug)]
pub struct ListenEntry { 
    pub port: u16,
    pub state: Arc<RwLock<TcpState>>
}
impl ListenEntry {
    pub fn new(port: u16) -> ListenEntry {
        ListenEntry { port, state: Arc::new(RwLock::new(TcpState::Listening)) }
    }
}

#[derive(Debug)]
pub struct ListenerEntry {
    pub accepting: bool,
    pub pending_connections: Vec<PendingConn>
}
impl ListenerEntry {
    pub fn new() -> ListenerEntry {
        ListenerEntry { accepting: false, pending_connections: Vec::new() }
    }
}

#[derive(Debug)]
pub struct PendingConn {
    pub sock: ConnectionSocket, //Connection socket that has not been run yet - run it when the associated listener socket starts accepting connections
}

impl PendingConn {
    pub fn new(sock: ConnectionSocket) -> PendingConn {
        PendingConn { sock }
    }
    /// Takes in a pending connection and adds it to the SocketTable before returning a pointer to that socket
    pub fn start(self, socket_table: &mut RwLockWriteGuard<SocketTable>) -> Arc<Mutex<ConnectionSocket>> {
        //Create entry on socket table and add it
        let sid = socket_table.len() as SocketId;
        let src_addr = (&self.sock.src_addr).clone();
        let dst_addr = (&self.sock.dst_addr).clone();
        let state = Arc::clone(&self.sock.state);
        let sock = Arc::new(Mutex::new(self.sock));
        let ret_clone = Arc::clone(&sock);
        let ent = ConnectionEntry { src_addr, dst_addr, state, sock };
        let ent = SocketEntry::Connection(ent);
        socket_table.insert(sid, ent);
        ret_clone
    }
}