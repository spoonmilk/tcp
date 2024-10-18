use library::ip_data_types::{Node, NodeType, CmdType};
use std::collections::HashMap;
use std::{self, sync::mpsc::{channel, Sender}};
use std::thread;
use std::io::{self, Write};

fn main() {
    let nd = Node::new(NodeType::Host, vec![], HashMap::new(), HashMap::new(), HashMap::new());
    let (send, recv) = channel();
    send.send(CmdType::Ln).unwrap();
    thread::spawn(move || nd.run(recv));
    listen(send);
}
fn listen(send: Sender<CmdType>) -> () {
    loop {
        println!("Enter a command number: ");
        //io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).expect("Failed to read line");
            
        if input.trim() == "1" {
            let li_cmd: CmdType = CmdType::Li;
            match send.send(li_cmd) {
                Ok(_) => println!("Sent command without error"),
                Err(e) => eprintln!("Encountered error while sending: {e:?}")
            };
        }
    }
}
