use easy_repl::{command, CommandStatus, Repl};
use easy_repl::anyhow::{self, Context};
use std::env::Args;
use library::ip_data_types::{NodeType, CmdType};
use tokio::sync::mpsc::Sender;

#[tokio::main]
pub async fn run_repl(_n_type: NodeType, send_nchan: Sender<CmdType>) {
    #[rustfmt::skip]
    let mut repl = Repl::builder()
        .add("li", command! {
            "li: List interfaces",
            () => || {
                let ls_cmd: CmdType = CmdType::Li;
                send_cmd(ls_cmd, send_nchan);
                Ok(CommandStatus::Done)
            }
        })
        .add("ln", command! {
            "ln: List neighbors",
            () => || {
                // Logic for listing neighbors
                eprintln!("If this was complete, you'd see a list of neighbors here!");
                Ok(CommandStatus::Done)
            }
        })
        .add("lr", command! {
            "lr: List routes",
            () => || {
                // Logic for listing routes
                eprintln!("If this was complete, you'd see a list of routes here!");
                Ok(CommandStatus::Done)
            }
        })
        .add("down", command! {
            "down: disable interface <ifname>",
            (ifname: String) => |ifname| {
                // Logic for listing interfaces
                eprintln!("Downing interfaces not yet implemented");
                Ok(CommandStatus::Done)
            }
        })
        .add("up", command! {
            "up: enable interface <ifname>",
            (ifname: String) => |ifname| {
                // Logic for listing interfaces
                eprintln!("Enabling interfaces not yet implemented");
                Ok(CommandStatus::Done)
            }
        })
        .add("send", command! {
            "send: Send a test packet",
            (addr: String, message: String) => |addr, message| {
                // Logic for listing interfaces
                eprintln!("Sending test packets not yet implemented");
                let i = 2;
                let mess: String = String::from("");
                while Args[i] != "\n" {
                    mess.push_str(Args[i]);
                }
                // Send packet with message
                Ok(CommandStatus::Done)
            }
        }).build()
        .context("Failed to create repl")?;

        repl.run().context("Critical REPL error")?;
}

async fn send_cmd(command: CmdType, send_nchan: Sender<CmdType>) {
    match send_nchan.send(command).await() {
        Err(e) => eprintln!("Error: Encountered error while sending command to node: {}", e)
        _ => (),
    }
}