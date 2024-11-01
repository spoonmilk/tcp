use crate::repl_trait::*;

const BASE_COMMANDS: Vec<(String, CommandData)> = vec![
    "li", CommandData { handler: |_args: Vec<String>| self.backend.li(), num_args: NumArgs::Exactly(0) }, 
    "ln", CommandData { handler: |_args: Vec<String>| self.backend.ln(), num_args: NumArgs::Exactly(0) }, 
    "lr", CommandData { handler: |_args: Vec<String>| self.backend.lr(), num_args: NumArgs::Exactly(0) },
    "up", CommandData { handler: |args: Vec<String>| self.backend.up(args.remove(0)), num_args: NumArgs::Exactly(1) },
    "down", CommandData { handler: |args: Vec<String>| self.backend.down(args.remove(0)), num_args: NumArgs::Exactly(1) },
    "send", CommandData { handler: |args: Vec<String>| {
        let parsed = <HostRepl as VnodeRepl>::parse_send(&mut args);
        let dst_ip = match Ipv4Addr::try_from(parsed.0) {
            Ok(ip_addr) => ip_addr,
            Err(_) => return Err("Input IP address is not a valid IP address")
        };
        let pb = PacketBasis { dst_ip, prot_num: 0, msg: parsed.1 };
        backend.raw_send(pb)
    }, num_args: NumArgs::Any },
];

pub struct HostRepl {
    pub backend: Backend,
    command_table: CommandTable
}

impl VnodeRepl for HostRepl {
    fn backend(&self) -> &Backend { &self.backend }
    fn command_table(&self) -> &CommandTable { &self.command_table }
    fn command_table_mut(&self) -> &mut CommandTable { &mut self.command_table }
    fn get_all_commands(&self) -> Vec<(String, CommandData)> {
        BASE_COMMANDS.clone() // + other commands - COMING SOON
    }
}

impl HostRepl {
    pub fn new(backend: Backend) {
        HostRepl { backend, command_table: HashMap::new() }
    }
}

pub struct RouterRepl {
    pub backend: Backend,
    command_table: CommandTable
}

impl VnodeRepl for RouterRepl {
    fn backend(&self) -> &Backend { &self.backend }
    fn command_table(&self) -> &CommandTable { &self.command_table }
    fn command_table_mut(&self) -> &mut CommandTable { &mut self.command_table }
    fn get_all_commands(&self) -> Vec<(String, CommandData)> { BASE_COMMANDS.clone() }
}

impl RouterRepl {
    pub fn new(backend: Backend) {
        RouterRepl { backend, command_table: HashMap::new() }
    }
}