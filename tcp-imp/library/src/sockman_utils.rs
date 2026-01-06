use crate::conn_socket::ConnectionSocket;
use crate::prelude::*;
use crate::tcp_utils::*;

pub type SocketId = u16;
pub type SocketTable = HashMap<SocketId, SocketEntry>;
pub type ListenerTable = HashMap<u16, ListenerEntry>;

pub struct SidAssigner {
    next_sid: AtomicU16,
}
impl Default for SidAssigner {
    fn default() -> Self {
        Self::new()
    }
}

impl SidAssigner {
    pub fn new() -> SidAssigner {
        SidAssigner {
            next_sid: AtomicU16::new(0),
        }
    }
    pub fn assign_sid(&self) -> SocketId {
        let sid = self.next_sid.load(Ordering::SeqCst);
        self.next_sid.store(sid + 1, Ordering::SeqCst);
        sid
    }
}

#[derive(Debug, Clone)]
pub enum SocketEntry {
    Connection(ConnectionEntry),
    Listener(ListenEntry),
}

#[derive(Debug, Clone)]
pub struct ConnectionEntry {
    pub src_addr: TcpAddress,
    pub dst_addr: TcpAddress,
    pub state: Arc<RwLock<TcpState>>,
    //pub sender: Sender<SocketCmd>
    pub sock: Arc<Mutex<ConnectionSocket>>,
}

#[derive(Debug, Clone)]
pub struct ListenEntry {
    pub port: u16,
    pub state: Arc<RwLock<TcpState>>,
}
impl ListenEntry {
    pub fn new(port: u16) -> ListenEntry {
        ListenEntry {
            port,
            state: Arc::new(RwLock::new(TcpState::Listening)),
        }
    }
}

#[derive(Debug)]
pub struct ListenerEntry {
    pub accepting: bool,
    pub pending_connections: Vec<PendingConn>,
    pub sock_send: Option<Sender<Arc<Mutex<ConnectionSocket>>>>, //This is so cursed wtf
}
impl Default for ListenerEntry {
    fn default() -> Self {
        Self::new()
    }
}

impl ListenerEntry {
    pub fn new() -> ListenerEntry {
        ListenerEntry {
            accepting: false,
            pending_connections: Vec::new(),
            sock_send: None, //Initially None - will become Some(<sender>) when accept1() gets called
        }
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
    pub fn start(
        self,
        socket_table: &mut RwLockWriteGuard<SocketTable>,
        sid: SocketId,
    ) -> Arc<Mutex<ConnectionSocket>> {
        //Create entry on socket table and add it
        let src_addr = self.sock.src_addr.clone();
        let dst_addr = self.sock.dst_addr.clone();
        let state = Arc::clone(&self.sock.state);
        // let sock = self.sock.run();
        let sock = Arc::new(Mutex::new(self.sock));
        ConnectionSocket::set_sid(Arc::clone(&sock), sid); //Socket needs to know its own ID
        let ret_clone = Arc::clone(&sock);
        // Spawn thread for timeouts
        let time_clone = Arc::clone(&sock);
        thread::spawn(move || {
            ConnectionSocket::time_check(time_clone);
        });
        let ent = ConnectionEntry {
            src_addr,
            dst_addr,
            state,
            sock,
        };
        let ent = SocketEntry::Connection(ent);
        socket_table.insert(sid, ent);
        ret_clone
    }
}
