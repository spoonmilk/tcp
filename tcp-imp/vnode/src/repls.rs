use std::collections::HashMap;
use crate::repl_trait::*;
use library::backends::{HostBackend, RouterBackend};
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
        self.get_base_commands() // + other commands - COMING SOON
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