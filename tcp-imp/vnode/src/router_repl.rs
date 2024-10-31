use rustyline::{error::ReadlineError, history::DefaultHistory, Editor};
use std::result;
use library::backends::RouterBackend;
use library::vnode_traits::VnodeBackend;
use library::utils::PacketBasis;
use std::sync::mpsc::Receiver;

pub fn run_app(backend: RouterBackend, ip_recver: Receiver<String>) -> () {
    thread::spawn(move || run_repl(backend));
    ip_listen(ip_recver);
}

fn ip_listen(ip_recver: Receiver<String>) -> () {
    loop {
        match ip_recver.recv() {
            Ok(msg) => println!("{msg:?}"),
            Err(e) => panic!("Error receiving from ip channel")
        }
    }
}

fn run_repl(backend: RouterBackend) -> () {
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
            backend.send(parsed.0, parsed.1)
        }
        _ => return Err(String::from(format!("\"{cmd:?}\" is not a valid command"))),
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
}
