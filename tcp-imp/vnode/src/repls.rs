use std::collections::HashMap;
use std::net::Ipv4Addr;
use crate::repl_trait::*;
use library::backends::{HostBackend, RouterBackend};
use library::socket_manager::SocketEntry;
use std::thread;
use std::sync::mpsc::Receiver;

pub struct HostRepl {
    pub backend: HostBackend,
    command_table: CommandTable
}

impl VnodeRepl<HostBackend> for HostRepl {
    fn backend(&self) -> &HostBackend { &self.backend }
    fn command_table(&self) -> &CommandTable { &self.command_table }
    fn command_table_mut(&mut self) -> &mut CommandTable { &mut self.command_table }
    fn get_all_commands(&self) -> Vec<(String, CommandData)> {
        let custom_commands = vec![
            ("a", CommandData { handler: Self::li_handler, num_args: NumArgs::Exactly(1) }), 
            ("c", CommandData { handler: Self::ln_handler, num_args: NumArgs::Exactly(2) }), 
            ("ls", CommandData { handler: Self::lr_handler, num_args: NumArgs::Exactly(0) }),
        ];
        self.get_base_commands().iter().chain(custom_commands).cloned().collect()
    }
}

impl HostRepl {
    pub fn new(backend: HostBackend) -> HostRepl {
        HostRepl { backend, command_table: HashMap::new() }
    }
    pub fn run(mut self, ip_recver: Receiver<String>) -> () { //Can't be put in trait because of weird size issue
        self.init_command_table();
        thread::spawn(move || Self::ip_listen(ip_recver));
        self.run_repl();
    }
    //Extra
    pub fn a_handler(&self, mut args: Vec<String>) -> () {
        //Sanititize input
        let port = if let Ok(port) = u16::try_from(arg[0]) { port } else { return println!("Input port \"{}\" invalid", args[0]) };
        //Listen on a port and then immediately accept on that port
        self.backend().listen(port.clone());
        self.backend().accept(port);
    }
    pub fn c_handler(&self, mut args: Vec<String>) -> () {
        //Sanititze input
        let ip_addr = if let Ok(ip_addr) = Ipv4Addr::try_from(args[0]) { ip_addr } else { return println!("Input IP address \"{}\" invalid", args[0]) };
        let port = if let Ok(port) = Ipv4Addr::try_from(args[1]) { port } else { return println!("Input IP address \"{}\" invalid", args[1]) };
        //Connect on an ip and port
        self.backend().connect(ip_addr, port);
    }
    pub fn ls_handler(&self, _args: Vec<String>) -> () {
        let socket_table = self.backend().socket_table();
        println!("SID\tLAddr\t\tLPort\tRAddr\t\tRPort\tState");
        for (sid, ent) in &*socket_table {
            let to_print = match ent {
                SocketEntry::Connection(ent) => format!("{sid:?}\t{}\t\t{}\t{}\t\t{}\t{}", ent.src_addr.ip, ent.src_addr.port, ent.dst_addr.ip, ent.dst_addr.port, ent.state.read().unwrap()),
                SocketEntry::Listener(ent) => format!("{sid:?}\t*\t\t{}\t*\t\t*\t{}", ent.port, ent.state.read().unwrap())
            };
            println!(to_print);
        }
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
    pub fn run(mut self, ip_recver: Receiver<String>) -> () {
        self.init_command_table();
        thread::spawn(move || Self::ip_listen(ip_recver));
        self.run_repl();
    }
}