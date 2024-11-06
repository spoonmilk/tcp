use rustyline::{error::ReadlineError, history::DefaultHistory, Editor};
use std::result;
//use library::backends::Backend;
use library::vnode_traits::VnodeBackend;
use library::utils::PacketBasis;
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::net::Ipv4Addr;

//pub type CommandHandler = Box<dyn FnMut(Vec<String>) -> ()>;
//pub type CommandHandler = for<'a> fn(&'a dyn VnodeBackend, Vec<String>) -> ();
pub type CommandHandler = Box<dyn for<'a> Fn(&'a dyn VnodeBackend, Vec<String>)>;

pub enum NumArgs {
    Exactly(usize),
    Any
}

pub struct CommandData {
    pub handler: CommandHandler,
    pub num_args: NumArgs
}

pub type CommandTable = HashMap<String, CommandData>;

pub trait VnodeRepl<Backend: VnodeBackend + 'static> where Self: 'static {
    //Getters
    fn backend(&self) -> &Backend;
    fn command_table(&self) -> &CommandTable;
    fn command_table_mut(&mut self) -> &mut CommandTable;
    //Functions that differ between repls
    fn get_all_commands(&self) -> Vec<(String, CommandData)>;
    //fn ip_listen(ip_recver: Receiver<Packet>) -> ();
    //Methods
    fn init_command_table(&mut self) -> () {
        let mut commands = self.get_all_commands();
        let command_table = self.command_table_mut();
        commands.drain(..).for_each(|(name, cd)| { command_table.insert(name, cd); });
    }
    /* Unnecessary, I guess
    fn add_command(&mut self, name: String, num_args: NumArgs, handler: CommandHandler) -> () {
        let cd = CommandData { handler, num_args };
        self.command_table_mut().insert(name, cd);
    }*/
    fn run_repl(&self) -> () {
        let mut ed = Editor::<(), DefaultHistory>::new().unwrap();
        loop {
            let cmd = ed.readline("> ");
            match cmd {
                Ok(cmd) => {
                    if let Err(e) = self.execute_command(cmd) { println!("{e:?}"); }
                }
                Err(ReadlineError::Interrupted) => break println!("Exiting"),
                Err(e) => eprintln!("{e:?}"),
            }
        }
    }
    fn execute_command(&self, cmd: String) -> result::Result<(), String> {
        //Parse input into a command and list of arguments
        let mut split_cmd: Vec<&str> = cmd.trim().split_whitespace().collect();
        if split_cmd.is_empty() { return Err(String::from("No command input")); }
        let cmd = split_cmd.remove(0).into();
        let args = split_cmd.iter().map(|&s| s.to_string()).collect();
        //Handle command
        self.handle_cmd(cmd, args)
    }
    fn handle_cmd(&self, cmd: String, args: Vec<String>) -> result::Result<(), String> {
        let command_table = self.command_table();
        let cmd_data = match command_table.get(&cmd) {
            Some(cd) => cd,
            None => return Err(format!("Invalid command: {cmd:?}"))
        };
        match cmd_data.num_args {
            NumArgs::Exactly(num) if num == args.len() => {},
            NumArgs::Any if args.len() > 0 => {},
            _ => return Err(format!("Improper number of arguments for {cmd:?}"))
        }
        let backend = self.backend();
        (cmd_data.handler)(backend, args);
        Ok(())
    }
    fn parse_send(args: &mut Vec<String>) -> (String, String) {
        // Get address from first message
        let addr = args.remove(0);
        let mut rest: String = String::from("");
    
        for s in args.iter() {
            if s.contains("\n") {
                return (addr, rest);
            }
            if !rest.is_empty() {
                rest.push_str(" ");
            }
            rest.push_str(s);
        }
        return (addr, rest);
    }
    fn get_base_commands(&self) -> Vec<(String, CommandData)> {
        let mut base_commands = vec![
            ("li", CommandData { handler: Box::new(Self::li_handler), num_args: NumArgs::Exactly(0) }), 
            ("ln", CommandData { handler: Box::new(Self::ln_handler), num_args: NumArgs::Exactly(0) }), 
            ("lr", CommandData { handler: Box::new(Self::lr_handler), num_args: NumArgs::Exactly(0) }),
            ("up", CommandData { handler: Box::new(Self::up_handler), num_args: NumArgs::Exactly(1) }),
            ("down", CommandData { handler: Box::new(Self::down_handler), num_args: NumArgs::Exactly(1) }),
            ("send", CommandData { handler: Box::new(Self::send_handler), num_args: NumArgs::Any }),
        ];
        base_commands.drain(..).map(|(name, cd)| (name.to_string(), cd)).collect()
    }
    //BASE COMMAND HANDLERS
    fn li_handler(backend: &dyn VnodeBackend, _args: Vec<String>) { backend.li() }
    fn ln_handler(backend: &dyn VnodeBackend, _args: Vec<String>) { backend.ln() }
    fn lr_handler(backend: &dyn VnodeBackend, _args: Vec<String>) { backend.lr() }
    fn up_handler(backend: &dyn VnodeBackend, mut args: Vec<String>) { backend.up(args.remove(0)) }
    fn down_handler(backend: &dyn VnodeBackend, mut args: Vec<String>) { backend.down(args.remove(0)) }
    fn send_handler(backend: &dyn VnodeBackend, mut args: Vec<String>) {  
        let parsed = Self::parse_send(&mut args);
        let dst_ip = match parsed.0.parse::<Ipv4Addr>() {
            Ok(ip_addr) => ip_addr,
            Err(_) => return eprintln!("Input IP address is not a valid IP address")
        };
        let pb = PacketBasis { dst_ip, prot_num: 0, msg: parsed.1.as_bytes().to_vec() };
        backend.raw_send(pb)
    }
    //UTILITY - should really be contained only in IpHandler but nope, for backwards compatability
    fn string_ip(raw_ip: [u8; 4]) -> String {
        Vec::from(raw_ip)
            .iter()
            .map(|num| num.to_string())
            .collect::<Vec<String>>()
            .join(".")
    }
}


/*
fn execute_command(cmd: String, backend: &RouterBackend) -> result::Result<(), String> {
    //Parse input into a command and list of arguments
    let mut split_cmd: Vec<&str> = cmd.trim().split_whitespace().collect();
    if split_cmd.is_empty() { return Err(String::from("No command input")); }
    let cmd = split_cmd.remove(0);
    let mut args = split_cmd.iter().map(|&s| s.to_string()).collect();
    //Ensure command + arguments is valid (proper number of arguments)

    if let Err(e) = validate_args(cmd, &args) { return Err(e); }
    //Execute the proper command
    match cmd {
        "li" => backend.li(),
        "ln" => backend.ln(),
        "lr" => backend.lr(),
        "up" => backend.up(args.remove(0)),
        "down" => backend.down(args.remove(0)),
        "send" => {
            let parsed = parse_send(&mut args);
            let dst_ip = match Ipv4Addr::try_from(parsed.0) {
                Ok(ip_addr) => ip_addr,
                Err(_) => return Err(format!("Input IP address is not a valid IP address"))
            };
            let pb = PacketBasis { dst_ip, prot_num: 0, msg: parsed.1 };
            backend.raw_send(pb)
        }
        _ => return Err(format!("\"{cmd:?}\" is not a valid command")),
    };
    Ok(())
}



fn parse_send(args: &mut Vec<String>) -> (String, String) {
    // Get address from first message
    let addr = args.remove(0);
    let mut rest: String = String::from("");

    for s in args.iter() {
        if s.contains("\n") {
            return (addr, rest);
        }
        if !rest.is_empty() {
            rest.push_str(" ");
        }
        rest.push_str(s);
    }
    return (addr, rest);
}

/// Make sure command has valid amount of arguments
fn validate_args(cmd: &str, args: &Vec<String>) -> result::Result<(), String> {
    match cmd {
        "li" => {
            if args.len() != 0 {
                Err(String::from("li takes no arguments"))
            } else {
                Ok(())
            }
        }
        "ln" => {
            if args.len() != 0 {
                Err(String::from("ln takes no arguments"))
            } else {
                Ok(())
            }
        }
        "lr" => {
            if args.len() != 0 {
                Err(String::from("lr takes no arguments"))
            } else {
                Ok(())
            }
        }
        "up" => {
            if args.len() != 1 {
                Err(String::from("Proper command format: up <ifname>"))
            } else {
                Ok(())
            }
        }
        "down" => {
            if args.len() != 1 {
                Err(String::from("Proper command format: down <ifname>"))
            } else {
                Ok(())
            }
        }
        "send" => {
            if args.len() < 2 {
                Err(String::from(
                    "Proper command format: send <addr> <message...>",
                ))
            } else {
                Ok(())
            }
        }
        _ => Err(String::from("Invalid command")),
    }
}*/
