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
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::io::ErrorKind;

//TODO:
//Closing listener sockets doesn't work - DONE
//Make receiving a FIN increment the ack number - DONE
//TimeWait needs to be fully implemented - DONE
//Handshake timeout
//Have close functionality vary based on current state - DONE
//Test retransmissions and ZWP together and with closing (test send/receive file)
//Run performance test

const READ_CHUNK: usize = 1380;

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
            ("sf".to_string(), CommandData { handler: Self::wrap_host_handler(Self::sf_handler), num_args: NumArgs::Exactly(3) }),
            ("rf".to_string(), CommandData { handler: Self::wrap_host_handler(Self::rf_handler), num_args: NumArgs::Exactly(2) }),
            ("cl".to_string(), CommandData { handler: Self::wrap_host_handler(Self::cl_handler), num_args: NumArgs::Exactly(1) })
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
        //Sanitize input
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
        let data = <String as Clone>::clone(&args[1]).into_bytes();
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
                    Err(_) => panic!("Received non utf8 encoded data :(")
                };
                println!("Received {} bytes. As a string, they are:\n{}", msg.len(), msg)
            },
            Err(e) => println!("{}", e.to_string())
        }
    }
    pub fn sf_handler(backend: &HostBackend, args: Vec<String>) -> () {
        //Sanitize input 
        let filepath: PathBuf = {
            if let Ok(path) = Path::new(&args[0]).canonicalize() {
                // Check that inputted filepath exists and is a file/not a directory
                if path.exists() && path.is_file() {
                    path
                } else {
                    return eprintln!("Input file path {} does not exist or is not a file", args[0]);
                }
            } else {
                return eprintln!("Input file path {} invalid", args[0]);
            }
        };
        let ip_addr = if let Ok(ip_addr) = args[1].parse::<Ipv4Addr>() { ip_addr } else { return eprintln!("Input IP address \"{}\" invalid", args[1]) };
        let port = if let Ok(port) = args[2].parse::<u16>() { port } else { return eprintln!("Input port \"{}\" invalid", args[2]) };
        //Spawn thread to do the actual sending
        let backend_clone = backend.clone(); //Again, kinda kludgy, but best we can do at the moment
        thread::spawn(move || Self::send_file(backend_clone, filepath, ip_addr, port));
        //Spawn a thread that...
        //First calls backend.connect() on ip address and port number
        //Then loops through... 
        //Reading 1kb of the file 
        //Calling backend.tcp_send() on the data and waiting for it to finish
        //Continues to do this until we've read the entire file
        //Closes the connection 
    }
    fn send_file(backend: HostBackend, filepath: PathBuf, ip_addr: Ipv4Addr, port: u16) {
        // Open the file
        let mut file = match File::open(filepath) {
            Ok(file) => file,
            Err(e) => return eprintln!("Unable to open file: {}", e)
        };
        // 1 kb buffer for reading into send
        let mut buf: Vec<u8> = vec![0u8; READ_CHUNK];
        // Call connect and establish a connection on the inputted ip and port
        let sid = backend.connect(ip_addr, port);
        //Let the sending begin!
        let mut total_bytes_sent: u32 = 0; 
        let mut iter_cnt = 0;
        loop {
            iter_cnt += 1;
            let bytes_read = file.read(&mut buf).unwrap();
            if bytes_read == 0 { break; } //We have reached EOF
            total_bytes_sent += match backend.tcp_send(sid, buf[..bytes_read].to_vec()) {
                Ok(bytes_sent) => bytes_sent,
                Err(e) => panic!("{}", e.to_string())
            };
            //  println!("Current total bytes sent: {}", total_bytes_sent);
        }
        backend.close(sid).expect("Somehow connection socket already got removed from socket table...");
        // println!("Iter count: {iter_cnt}");
        println!("Sent {total_bytes_sent} bytes");

    }
    pub fn rf_handler(backend: &HostBackend, args: Vec<String>) -> () {
        //Sanitize input
        let path = Path::new(&args[0]);
        let file = match File::create(path) {
            Ok(fl) => fl,
            Err(ref e) if e.kind() == ErrorKind::NotFound => return eprintln!("Invalid input path: {:?}", path),
            Err(e) => panic!("{e:?}")
        };
        let port = if let Ok(port) = args[1].parse::<u16>() { port } else { return eprintln!("Input port \"{}\" invalid", args[1]) };
        //Spawn a thread to complete the file reception
        let backend_clone = backend.clone(); //Kinda kludgy, but best I could come up with without major changes
        thread::spawn(move || Self::receive_file(backend_clone, file, port));
    }
    fn receive_file(backend: HostBackend, mut file: File, port: u16) {
        backend.listen(port);
        let sid =  backend.accept1(port).expect("No listener socket with input port found...");
        let mut total_bytes_read = 0;
        loop {
            let data = match backend.tcp_recieve(sid, READ_CHUNK as u16) {
                Ok(data) => data,
                Err(ref e) if e.kind() == ErrorKind::InvalidInput => panic!("Socket at sid we were given by accept1() doesn't exist..."),
                Err(ref e) if e.kind() == ErrorKind::Unsupported => break, //We can't receive anymore - this is expected behavior for when there is nothing left to receive
                Err(e) => panic!("Received error: {e:?}")
            };
            file.write_all(&data).unwrap();
            total_bytes_read += data.len();
        }
        println!("We're getting here...");
        backend.close(sid).expect("No socket at the sid we just got back from accept1()...");
        println!("Read {total_bytes_read} bytes");
    }
    pub fn cl_handler(backend: &HostBackend, args: Vec<String>) -> () {
        //Sanitize input
        let sid = if let Ok(sid) = args[0].parse::<SocketId>() { sid } else { return println!("Input socket ID {} invalid", args[0]) };
        //Make the backend close that socket
        if let Err(e) = backend.close(sid) { println!("{}", e.to_string())};
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