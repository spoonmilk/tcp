use rustyline::{error::ReadlineError, history::DefaultHistory, Editor};
use std::result;
use library::ip_data_types::{CmdType, NodeType};
use std::sync::mpsc::Sender; //{Arc, Mutex, mpsc::Sender};
struct NodeRep {
    _n_type: NodeType,
    send: Sender<CmdType>,
}

impl NodeRep {
    fn send_cmd(&self, cmd: CmdType) {
        match self.send.send(cmd) {
            Err(e) => eprintln!(
                "Error: Encountered error while sending command to node: {}",
                e
            ),
            _ => (),
        }
    }
}

pub fn run_repl(n_type: NodeType, send_nchan: Sender<CmdType>) -> () {
    let mut ed = Editor::<(), DefaultHistory>::new().unwrap();
    let nd_rep = NodeRep {
        _n_type: n_type,
        send: send_nchan,
    };
    loop {
        let cmd = ed.readline("> ");
        match cmd {
            Ok(cmd) => {
                if let Err(e) = execute_command(cmd, &nd_rep) {
                    println!("{e:?}");
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("Exiting");
                break;
            }
            Err(e) => eprintln!("{e:?}"),
        }
    }
}

fn execute_command(cmd: String, nd_rep: &NodeRep) -> result::Result<(), String> {
    let mut split_cmd: Vec<&str> = cmd.trim().split_whitespace().collect();
    if split_cmd.is_empty() {
        return Err(String::from("No command input"));
    }
    let cmd = split_cmd.remove(0);
    let mut args = split_cmd.iter().map(|&s| s.to_string()).collect();
    if let Err(e) = validate_args(cmd, &args) {
        return Err(e);
    }
    let cmd_to_send = match cmd {
        "li" => CmdType::Li,
        "ln" => CmdType::Ln,
        "lr" => CmdType::Lr,
        "up" => CmdType::Up(args.remove(0)),
        "down" => CmdType::Down(args.remove(0)),
        "send" => {
            let parsed = parse_send(&mut args);
            CmdType::Send(parsed.0, parsed.1)
        }
        _ => return Err(String::from("Improper number of arguments")), //Should never happen in practice
    };
    nd_rep.send_cmd(cmd_to_send);
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
