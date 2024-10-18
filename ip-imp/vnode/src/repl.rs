use easy_repl::{command, CommandStatus, Repl};
use easy_repl::anyhow::{self, Context};
use std::env;
use library::ip_data_types::{NodeType, CmdType};
use std::sync::mpsc::Sender;

pub fn run_repl(_n_type: NodeType, send_nchan: Sender<CmdType>) -> anyhow::Result<()> {
    let send_nchan_clone = send_nchan.clone();
    let mut repl = Repl::builder()
        .add("li", command! {
            "li: List interfaces",
            () => || {
                let ls_cmd: CmdType = CmdType::Li;
                send_cmd(ls_cmd, send_nchan_clone.clone());
                Ok(CommandStatus::Done)
            }
        })
        .add("ln", command! {
            "ln: List neighbors",
            () => || {
                let ln_cmd: CmdType = CmdType::Ln;
                send_cmd(ln_cmd, send_nchan_clone.clone());
                Ok(CommandStatus::Done)
            }
        })
        .add("lr", command! {
            "lr: List routes",
            () => || {
                let lr_cmd: CmdType = CmdType::Lr;
                send_cmd(lr_cmd, send_nchan_clone.clone());
                Ok(CommandStatus::Done)
            }
        })
        .add("down", command! {
            "down: disable interface <ifname>",
            (ifname: String) => |ifname| {
                let down_cmd: CmdType = CmdType::Down(ifname);
                send_cmd(down_cmd, send_nchan_clone.clone());
                Ok(CommandStatus::Done)
            }
        })
        .add("up", command! {
            "up: enable interface <ifname>",
            (ifname: String) => |ifname| {
                let up_cmd: CmdType = CmdType::Up(ifname);
                send_cmd(up_cmd, send_nchan_clone.clone());
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
                send_cmd(sender_cmd, send_nchan_clone.clone());
                // Send packet with message
                Ok(CommandStatus::Done)
            }
        })
        .build()
        .context("Failed to create repl")?;

    repl.run().context("Critical REPL error")
}

async fn send_cmd(command: CmdType, send_nchan: Sender<CmdType>) {
    match send_nchan.send(command).await {
        Err(e) => eprintln!("Error: Encountered error while sending command to node: {}", e),
        _ => (),
    }
}