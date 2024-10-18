/*use easy_repl::{command, CommandStatus, Repl};
use easy_repl::anyhow::{self, Context};*/
use rustyline::{Editor, history::DefaultHistory, error::ReadlineError};
use std::result;
//use std::env;
use library::ip_data_types::{CmdType, NodeType};
use std::sync::mpsc::Sender;//{Arc, Mutex, mpsc::Sender};

/*
pub fn run_repl(_n_type: NodeType, send_nchan: Sender<CmdType>) -> anyhow::Result<()> {
    let send_nchan_mut = Arc::new(Mutex::new(send_nchan));
    let mut repl = Repl::builder()
        .add("li", command! {
            "li: List interfaces",
            () => || {
                let ls_cmd: CmdType = CmdType::Li;
                let send_nchan_mut = Arc::clone(&send_nchan_mut);
                send_cmd(ls_cmd, send_nchan_mut);
                Ok(CommandStatus::Done)
            }
        })
        .add("ln", command! {
            "ln: List neighbors",
            () => || {
                let ln_cmd: CmdType = CmdType::Ln;
                let send_nchan_mut = Arc::clone(&send_nchan_mut);
                send_cmd(ln_cmd, send_nchan_mut);
                Ok(CommandStatus::Done)
            }
        })
        .add("lr", command! {
            "lr: List routes",
            () => || {
                let lr_cmd: CmdType = CmdType::Lr;
                let send_nchan_mut = Arc::clone(&send_nchan_mut);
                send_cmd(lr_cmd, send_nchan_mut);
                Ok(CommandStatus::Done)
            }
        })
        .add("down", command! {
            "down: disable interface <ifname>",
            (ifname: String) => |ifname| {
                let down_cmd: CmdType = CmdType::Down(ifname);
                let send_nchan_mut = Arc::clone(&send_nchan_mut);
                send_cmd(down_cmd, send_nchan_mut);
                Ok(CommandStatus::Done)
            }
        })
        .add("up", command! {
            "up: enable interface <ifname>",
            (ifname: String) => |ifname| {
                let up_cmd: CmdType = CmdType::Up(ifname);
                let send_nchan_mut = Arc::clone(&send_nchan_mut);
                send_cmd(up_cmd, send_nchan_mut);
                Ok(CommandStatus::Done)
            }
        })
        .add("send", command! {
            "send: Send a test packet",
            (addr: String, message: String) => |addr: String, message: String| {
                let i: usize = 2;
                let args: Vec<String> = env::args().collect();
                let retstr: String = String::from("");
                while i < args.len() {
                    retstr.push_str(&args[i]);
                    if args[i].contains("\n") {
                        break;
                    }
                }
                let sender_cmd: CmdType = CmdType::Send(addr, retstr);
                let send_nchan_mut = Arc::clone(&send_nchan_mut);
                send_cmd(sender_cmd, send_nchan_mut);
                // Send packet with message
                Ok(CommandStatus::Done)
            }
        })
        .build()
        .context("Failed to create repl")?;

    repl.run().context("Critical REPL error")
}

fn send_cmd(command: CmdType, send_nchan_mut: Arc<Mutex<Sender<CmdType>>>) {
    let send_nchan = send_nchan_mut.lock().unwrap();
    match send_nchan.send(command) {
        Err(e) => eprintln!("Error: Encountered error while sending command to node: {}", e),
        _ => (),
    }
}*/

struct NodeRep {
    _n_type: NodeType,
    send: Sender<CmdType>
}

impl NodeRep {
    fn send_cmd(&self, cmd: CmdType) {
        match self.send.send(cmd) {
            Err(e) => eprintln!("Error: Encountered error while sending command to node: {}", e),
            _ => (),
        }
    }
}

pub fn run_repl(n_type: NodeType, send_nchan: Sender<CmdType>) -> () {
    let mut ed = Editor::<(), DefaultHistory>::new().unwrap();
    let nd_rep = NodeRep {
        _n_type: n_type,
        send: send_nchan
    };
    loop {
        let cmd = ed.readline("> ");
        match cmd {
            Ok(cmd) => if let Err(e) = execute_command(cmd, &nd_rep) { println!("{e:?}"); },
            Err(ReadlineError::Interrupted) => {
                println!("Exiting");
                break;
            },
            Err(e) => eprintln!("{e:?}")
        }
    }
}

fn execute_command(cmd: String, nd_rep: &NodeRep) -> result::Result<(), String> {
    let mut split_cmd: Vec<&str> = cmd.trim().split_whitespace().collect();
    if split_cmd.is_empty() {
        return Err(String::from("No command input"))
    }
    let cmd = split_cmd.remove(0);
    let mut args = split_cmd.iter().map(|&s| s.to_string()).collect();
    if let Err(e) = proper_num_args(cmd, &args) {
        return Err(e);
    }
    let cmd_to_send = match cmd {
        "li" => CmdType::Li,
        "ln" => CmdType::Ln,
        "lr" => CmdType::Lr,
        "up" => CmdType::Up(args.remove(0)),
        "down" => CmdType::Down(args.remove(0)),
        "send" => CmdType::Send(args.remove(0), args.remove(0)),
        _ => return Err(String::from("Improper number of arguments")) //Should never happen in practice
    };
    nd_rep.send_cmd(cmd_to_send);
    Ok(())
}

fn proper_num_args(cmd: &str, args: &Vec<String>) -> result::Result<(), String> {
    let proper_num = match cmd {
        "li" | "ln" | "lr" => 0,
        "up" | "down" => 1,
        "send" => 2,
        _ => return Err(String::from("Invalid command"))
    };
    if proper_num != args.len() {
        Err(String::from("Improper number of arguments"))
    } else {
        Ok(())
    }
}