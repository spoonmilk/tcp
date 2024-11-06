use crate::prelude::*;
use crate::utils::*;
use crate::tcp_utils::*;
use crate::socket_manager::*;
use crate::conn_socket::*;

pub struct IpHandler {
    socket_table: Arc<RwLock<SocketTable>>,
    socket_manager: Arc<Mutex<SocketManager>>,
}

impl IpHandler {
    pub fn new(socket_table: Arc<RwLock<SocketTable>>, socket_manager: Arc<Mutex<SocketManager>>) -> IpHandler {
        IpHandler { socket_table, socket_manager }
    }
    pub fn run(self, ip_recver: Receiver<Packet>) -> () {
        let pack = ip_recver.recv().expect("Error receiving from IP Daemon");
        match pack.header.protocol.0  {
            0 => Self::handle_test_packet(pack),
            6 => {
                let stable_clone = Arc::clone(&self.socket_table);
                let smanager_clone = Arc::clone(&self.socket_manager);
                thread::spawn(move || Self::handle_tcp_packet(pack, stable_clone, smanager_clone));
            }
            _ => println!("I don't know how to deal with packets of protocol number \"{}\"", pack.header.protocol.0)
        }
    }
    fn handle_tcp_packet(pack: Packet, socket_table: Arc<RwLock<SocketTable>>, socket_manager: Arc<Mutex<SocketManager>>) -> () {
        let tpack = deserialize_tcp(pack.data.clone()).expect("Malformed TCP packet");
        let socket_table = socket_table.read().unwrap();
        match Self::proper_socket(&pack.header, &tpack, &socket_table) {
            Some(sid) => {
                let sock_entry = socket_table.get(&sid).expect("Internal logic issue - check proper_socket");
                match sock_entry {
                    SocketEntry::Connection(ent) => {
                        let sock = Arc::clone(&ent.sock);
                        thread::spawn(move || ConnectionSocket::handle_packet(sock, tpack));
                    },
                    SocketEntry::Listener(ent) => { //Blegh, ownership
                        let port = ent.port.clone();
                        let sock_man = Arc::clone(&socket_manager);
                        thread::spawn(move || sock_man.lock().unwrap().handle_incoming(pack, port));
                    }
               }
            }
            None => {} //Drop packet cause nobody gives a crap about it
        }
    }
    ///Finds the proper socket for a TcpPacket given an associated IP header
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
    fn handle_test_packet(pack: Packet) -> () {
        let src = Self::string_ip(pack.header.source);
        let dst = Self::string_ip(pack.header.destination);
        let ttl = pack.header.time_to_live;
        // Message received is a test packet
        let msg = String::from_utf8(pack.data).unwrap();
        println!("Received tst packet: Src: {}, Dst: {}, TTL: {}, {}", src, dst, ttl, msg);
    }
    fn string_ip(raw_ip: [u8; 4]) -> String {
        Vec::from(raw_ip)
            .iter()
            .map(|num| num.to_string())
            .collect::<Vec<String>>()
            .join(".")
    }
}