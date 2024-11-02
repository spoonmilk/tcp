use crate::prelude::*;
use crate::tcp_utils::*;

pub struct ConnectionSocket {
    pub state: Arc<RwLock<TcpState>>,
    pub src_addr: TcpAddress,
    pub dst_addr: TcpAddress,
    backend_sender: Sender<String>,
    ip_sender: Sender<TcpPacket>
}

impl ConnectionSocket {
    pub fn new(state: Arc<RwLock<TcpState>>, src_addr: TcpAddress, dst_addr: TcpAddress, backend_sender: Sender<String>, ip_sender: Sender<TcpPacket>) -> ConnectionSocket {
        ConnectionSocket { state, src_addr, dst_addr, backend_sender, ip_sender }
    }
    pub fn run(self, sockman_recver: Receiver<SocketCmd>) -> () {
        loop {
            let command = sockman_recver.recv().expect("Error receiving from Socket Manager");
            let new_state = {
                let state = self.state.read().unwrap();
                match *state {
                    TcpState::Listening => panic!("I'm a freaking connection socket, why would I ever be in the listening state?!?"),
                    TcpState::SynSent => self.syn_sent_handle(command),
                    TcpState::SynRecvd => self.syn_recved_handle(command),
                    TcpState::Established => self.established_handle(command),
                    _ => panic!("I don't know what to do in these states yet!")
                }
            };
            let mut state = self.state.write().unwrap();
            *state = new_state;
        }
    }
    fn syn_sent_handle(&self, cmd: SocketCmd) -> TcpState {
        match cmd {
            SocketCmd::Process(_tpack) => {
                //Check if packet is syn + ack and all that jazz
            },
            SocketCmd::Send(_) | SocketCmd::Recv(_) => println!("I can't do that yet, not established"),
            _ => println!("I don't know that one yet *shrug*")
        }
        TcpState::SynSent
    }
    fn syn_recved_handle(&self, cmd: SocketCmd) -> TcpState {
        match cmd {
            SocketCmd::Process(_tpack) => {
                //Check if packet is ack and all that jazz
            },
            SocketCmd::Send(_) | SocketCmd::Recv(_) => println!("I can't do that yet, not established"),
            _ => println!("I don't know that one yet *shrug*")
        }
        TcpState::SynRecvd
    }
    fn established_handle(&self, cmd: SocketCmd) -> TcpState {
        match cmd {
            SocketCmd::Process(_tpack) => println!("I got a packet weee!"),
            SocketCmd::Send(_tpack) => println!("You want me to send a packet...? I dunno how to do that..."),
            SocketCmd::Recv(_bytes) => println!("Erm, I got nothing for ya"),
            _ => println!("I don't know that one yet *shrug*")
        }
        TcpState::Established
    }
}