use std::collections::HashMap;
use std::net::Ipv4Addr;
use crate::repl_trait::*;
use library::backends::{HostBackend, RouterBackend};
//use library::vnode_traits::VnodeBackend;
use library::sockman_utils::*;
use library::ip_handler::*;
use library::utils::*;
use library::vnode_traits::VnodeBackend; //Hopefully this can be removed in the future because this stuff shoud be private
use std::thread;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

pub struct HostRepl {
    pub backend: HostBackend,
    command_table: CommandTable
}

impl VnodeRepl<HostBackend> for HostRepl {
    fn backend(&self) -> &HostBackend { &self.backend }
    fn command_table(&self) -> &CommandTable { &self.command_table }
    fn command_table_mut(&mut self) -> &mut CommandTable { &mut self.command_table }
    fn get_all_commands(&self) -> Vec<(String, CommandData)> {
        let mut custom_commands = vec![
            ("a".to_string(), CommandData { handler: Self::wrap_host_handler(Self::a_handler), num_args: NumArgs::Exactly(1) }), 
            ("c".to_string(), CommandData { handler: Self::wrap_host_handler(Self::c_handler), num_args: NumArgs::Exactly(2) }), 
            ("ls".to_string(), CommandData { handler: Self::wrap_host_handler(Self::ls_handler), num_args: NumArgs::Exactly(0) }),
            ("s".to_string(), CommandData { handler: Self::wrap_host_handler(Self::s_handler), num_args: NumArgs::Exactly(2) }), 
            ("r".to_string(), CommandData { handler: Self::wrap_host_handler(Self::r_handler), num_args: NumArgs::Exactly(2) }),
        ];
        let mut all_commands = self.get_base_commands();
        all_commands.append(&mut custom_commands);
        all_commands
    }
}

impl HostRepl {
    pub fn new(backend: HostBackend) -> HostRepl {
        HostRepl { backend, command_table: HashMap::new() }
    }
    pub fn run(mut self, ip_recver: Receiver<Packet>) -> () { //Can't be put in trait because of weird size issue
        self.init_command_table();
        let backend = &self.backend;
        let socket_table = Arc::clone(&backend.socket_table);
        let socket_manager = Arc::clone(&backend.socket_manager);
        let ip_handler = IpHandler::new(socket_table, socket_manager);
        thread::spawn(move || ip_handler.run(ip_recver));
        self.run_repl();
    }
    //Additional command handlers
    pub fn a_handler(backend: &HostBackend, args: Vec<String>) -> () {
        //Sanititize input
        let port = if let Ok(port) = args[0].parse::<u16>() { port } else { return println!("Input port \"{}\" invalid", args[0]) };
        //Listen on a port and then immediately accept on that port
        backend.listen(port.clone());
        backend.accept(port);
    }

    pub fn c_handler(backend: &HostBackend, args: Vec<String>) -> () {
        //Sanititze input
        let ip_addr = if let Ok(ip_addr) = args[0].parse::<Ipv4Addr>() { ip_addr } else { return println!("Input IP address \"{}\" invalid", args[0]) };
        let port = if let Ok(port) = args[1].parse::<u16>() { port } else { return println!("Input IP address \"{}\" invalid", args[1]) };
        //Connect on an ip and port
        backend.connect(ip_addr, port);
    }
    pub fn ls_handler(backend: &HostBackend, _args: Vec<String>) -> () {
        let socket_table = backend.socket_table();
        println!("SID\tLAddr\t\tLPort\tRAddr\t\tRPort\tState");
        for (sid, ent) in &*socket_table {
            let to_print = match ent {
                SocketEntry::Connection(ent) => format!("{sid:?}\t{}\t{}\t{}\t{}\t{:?}", ent.src_addr.ip, ent.src_addr.port, ent.dst_addr.ip, ent.dst_addr.port, ent.state.read().unwrap()),
                SocketEntry::Listener(ent) => format!("{sid:?}\t*\t\t{}\t*\t\t*\t{:?}", ent.port, ent.state.read().unwrap())
            };
            println!("{}", to_print);
        }
    }
    pub fn s_handler(backend: &HostBackend, args: Vec<String>) -> () {
        //Sanitize input
        let sid = if let Ok(sid) = args[0].parse::<SocketId>() { sid } else { return println!("Input socket ID {} invalid", args[0]) };
        let data = if let Ok(data) = args[1].parse::<Vec<u8>>() { data } else { return println!("Input message unparsable to bytes???", args[1]) };
        //Send data and print result
        match backend.tcp_send(sid, data) {
            Ok(bytes_sent) => println!("Sent {bytes_sent} bytes"),
            Err(e) => println!("{}", e.to_string())
        }
    }
    pub fn r_handler(backend: &HostBackend, args: Vec<String>) -> () {
        //Sanitize input
        let sid = if let Ok(sid) = args[0].parse::<SocketId>() { sid } else { return println!("Input socket ID {} invalid", args[0]) };
        let bytes = if let Ok(bytes) = args[1].parse::<u16>() { bytes } else { return println!("Input number of bytes to read {} cannot parse to u16 - make sure it's less than 2^16", args[1]) };
        //Receive data and parse to string, printing the results
        match backend.tcp_recieve(sid, bytes) {
            Ok(data) => {
                let msg = match String::from_utf8(data) {
                    Ok(msg) => msg,
                    Err(_) => println!("Received non utf8 encoded data :(")
                };
                println!("Received {} bytes. As a string, they are:\n{}", msg.len(), msg)
            },
            Err(e) => println!("{}", e.to_string())
        }
    }
    fn wrap_host_handler<F>(f: F) -> CommandHandler
    where
        F: Fn(&HostBackend, Vec<String>) + 'static,
    {
        Box::new(move |backend: &dyn VnodeBackend, args: Vec<String>| {
            if let Some(host_backend) = backend.as_any().downcast_ref::<HostBackend>() {
                f(host_backend, args);
            } else {
                eprintln!("Invalid backend type for this command");
            }
        })
    }
}

pub struct RouterRepl {
    pub backend: RouterBackend,
    command_table: CommandTable
}

impl VnodeRepl<RouterBackend> for RouterRepl {
    fn backend(&self) -> &RouterBackend { &self.backend }
    fn command_table(&self) -> &CommandTable { &self.command_table }
    fn command_table_mut(&mut self) -> &mut CommandTable { &mut self.command_table }
    fn get_all_commands(&self) -> Vec<(String, CommandData)> { self.get_base_commands() }
}

impl RouterRepl {
    pub fn new(backend: RouterBackend) -> RouterRepl {
        RouterRepl { backend, command_table: HashMap::new() }
    }
    pub fn run(mut self, ip_recver: Receiver<Packet>) -> () {
        self.init_command_table();
        thread::spawn(move || Self::ip_listen(ip_recver));
        self.run_repl();
    }
    fn ip_listen(ip_recver: Receiver<Packet>) -> () {
        loop {
            let pack = ip_recver.recv().expect("Error receiving packet from IP Daemon");
            let src = Self::string_ip(pack.header.source);
            let dst = Self::string_ip(pack.header.destination);
            let ttl = pack.header.time_to_live;
            // Message received is a test packet
            let msg = String::from_utf8(pack.data).unwrap();
            println!("Received tst packet: Src: {}, Dst: {}, TTL: {}, {}", src, dst, ttl, msg);
        }
    }
}