use rustyline::{error::ReadlineError, history::DefaultHistory, Editor};
use std::result;
use library::backends::RouterBackend;
use library::vnode_traits::VnodeBackend;
use std::sync::mpsc::Sender; //{Arc, Mutex, mpsc::Sender};

pub fn run_app(backend: RouterBackend, ip_chan: BiChan<PacketBasis, String>) -> () {
    thread::spawn(|| run_repl)
}

pub fn run_repl(backend: RouterBackend, ip_sender: Sender<PacketBasis>) -> () {
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
        "a" => {
            if args.len() != 1 {
                Err(String::from(
                    "Proper command format: a <port>",
                ))
            } else {
                Ok(())
            }
        }
        "s" => {
            if args.len() != 1 {
                Err(String::from(
                    "Proper command format: s <socket ID> <bytes>",
                ))
            } else {
                Ok(())
            }
        }
        "r" => {
            if args.len() != 2 {
                Err(String::from(
                    "Proper command format: a <socket ID> <numbytes>",
                ))
            } else {
                Ok(())
            }
        }
        "ls" => {
            if args.len() != 0 {
                Err(String::from(
                    "ls takes no arguments",
                ))
            } else {
                Ok(())
            }
        }
        "cl" => {
            if args.len() != 1 {
                Err(String::from(
                    "Proper command format: cl <socket ID>",
                ))
            } else {
                Ok(())
            }
        }
        "sf" => {
            if args.len() != 3 {
                Err(String::from(
                    "Proper command format: sf <file path> <addr> <port>",
                ))
            } else {
                Ok(())
            }
        }
        "rf" => {
            if args.len() != 2 {
                Err(String::from(
                    "Proper command format: rf <dest file> <port>",
                ))
            } else {
                Ok(())
            }
        }
        _ => Err(String::from("Invalid command")),
    }
}
