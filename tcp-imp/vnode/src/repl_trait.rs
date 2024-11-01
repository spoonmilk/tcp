use rustyline::{error::ReadlineError, history::DefaultHistory, Editor};
use std::result;
//use library::backends::Backend;
use library::vnode_traits::VnodeBackend;
use library::utils::PacketBasis;
use std::sync::mpsc::Receiver;
use std::thread;

pub type Backend<T: VnodeBackend> = T;
pub type CommandHandler = fn (backend: &Backend, args: Vec<String>) -> ();

pub enum NumArgs {
    Exactly(usize),
    Any
}

pub struct CommandData {
    pub handler: CommandHandler,
    pub num_args: NumArgs
}

pub type CommandTable = HashMap<String, CommandData>;

pub trait VnodeRepl {
    //Getters
    fn backend(&self) -> &Backend;
    fn command_table(&self) -> &CommandTable;
    fn command_table_mut(&self) -> &mut CommandTable;
    //Functions that differ between repls
    fn get_all_commands(&self) -> Vec<(String, CommandData)>;
    //Methods
    pub fn run(ip_recver: Receiver<String>) -> () {
        let command_table = self.command_table_mut();
        self.get_all_commands().iter().for_each(|(name, cd)| command_table.insert(name, cd));
        thread::spawn(move || ip_listen(ip_recver));
        self.run_repl();
    }
    fn add_command(&mut self, name: String, num_args: NumArgs, handler: CommandHandler) -> () {
        let cd = CommandData { handler, num_args };
        self.command_table_mut().insert(name, cd);
    }
    fn ip_listen(ip_recver: Receiver<String>) -> () {
        loop {
            match ip_recver.recv() {
                Ok(msg) => println!("{msg:?}"),
                Err(e) => panic!("Error receiving from ip channel")
            }
        }
    }
    fn run_repl(&self) -> () {
        let mut ed = Editor::<(), DefaultHistory>::new().unwrap();
        loop {
            let cmd = ed.readline("> ");
            match cmd {
                Ok(cmd) => {
                    if let Err(e) = execute_command(cmd, &nd_rep) { println!("{e:?}"); }
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
        let mut args = split_cmd.iter().map(|&s| s.to_string()).collect();
        //Handle command
        self.handle_cmd(cmd, args)
    }
    fn handle_cmd(&self, cmd: String, args: Vec<String>) -> result::Result<(), String> {
        let command_table = self.command_table();
        let cmd_data: CommandData = match command_table.get(&cmd) {
            Some(cd) => cd,
            None => return Err(format!("Invalid command: {cmd:?}"))
        };
        match cmd_data.num_args {
            NumArgs::Exactly(num) if num == args.len() => {},
            NumArgs::Any if args.len() > 0 => {},
            _ => return Err(format!("Improper number of arguments for {cmd:?}"))
        }
        let backend = self.backend();
        cmd_data.handler(backend, args);
        Ok(())
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
