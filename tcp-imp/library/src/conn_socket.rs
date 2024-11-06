use crate::prelude::*;
use crate::utils::*;
use crate::tcp_utils::*;

pub struct ConnectionSocket {
    pub state: Arc<RwLock<TcpState>>,
    pub src_addr: TcpAddress,
    pub dst_addr: TcpAddress,
    backend_sender: Sender<String>,
    ip_sender: Sender<PacketBasis>,
    seq_num: u32,
    ack_num: u32,
    win_size: u16
}

impl ConnectionSocket {
    pub fn new(state: Arc<RwLock<TcpState>>, src_addr: TcpAddress, dst_addr: TcpAddress, backend_sender: Sender<String>, ip_sender: Sender<PacketBasis>) -> ConnectionSocket {
        let mut rand_rng = rand::thread_rng();
        let seq_num = rand_rng.gen::<u32>()/2;
        ConnectionSocket { state, src_addr, dst_addr, backend_sender, ip_sender, seq_num, ack_num: 0, win_size: 5000 }
    }
    /// Runs an initialized TCP connection socket
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
            SocketCmd::Process(tpack) if has_only_flags(&tpack.header, SYN | ACK) => {
                let new_pack = self.build_packet(Vec::new(), ACK);
                let pbasis = self.packet_basis(new_pack);
                self.ip_sender.send(pbasis).expect("Error sending packet to IP Daemon");
                return TcpState::Established;
            }
            SocketCmd::Process(_) => eprintln!("Received packet in SynSent state without proper flags SYN + ACK"),
            SocketCmd::Send(_) | SocketCmd::Recv(_) => println!("I can't do that yet, not established"),
            _ => println!("I don't know that one yet *shrug*")
        }
        TcpState::SynSent
    }
    fn syn_recved_handle(&self, cmd: SocketCmd) -> TcpState {
        match cmd {
            SocketCmd::Process(tpack) if has_only_flags(&tpack.header, ACK) => {
                println!("Received final ack, TCP handshake succesful!");
                //Check if packet is ack and all that jazz
                let mut slf_state = self.state.write().unwrap();
                *slf_state = TcpState::Established; 
            },
            SocketCmd::Process(_) => eprintln!("Received packet in SynRecvd state without proper flags ACK"),
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
    fn build_packet(&self, payload: Vec<u8>, flags: u8) -> TcpPacket {
        let mut tcp_header = TcpHeader::new(
            self.src_addr.port.clone(),
            self.dst_addr.port.clone(),
            self.seq_num.clone(),
            self.win_size.clone()
        );
        tcp_header.acknowledgment_number = self.ack_num.clone();
        ConnectionSocket::set_flags(&mut tcp_header, flags);
        let src_ip = self.src_addr.ip.clone().octets();
        let dst_ip = self.dst_addr.ip.clone().octets();
        let checksum = tcp_header.calc_checksum_ipv4_raw(src_ip, dst_ip, payload.as_slice()).expect("Checksum calculation failed");
        tcp_header.checksum = checksum;
        return TcpPacket { header: tcp_header, payload };
    }
    /// Takes in a TCP header and a u8 representing flags and sets the corresponding flags in the header.
    fn set_flags(head: &mut TcpHeader, flags: u8) -> () {
        if flags & SYN != 0 {
            head.syn = true;
        } else if flags & ACK != 0 {
            head.ack = true;
        } else if flags & FIN != 0 {
            head.fin = true;
        } else if flags & RST != 0 {
            head.rst = true;
        } else if flags & PSH != 0 {
            head.psh = true;
        } else if flags & URG != 0 {
            head.urg = true;
        }
    } 
    /// Takes in a TCP packet and outputs a Packet Basis for its IP packet
    fn packet_basis(&self, tpack: TcpPacket) -> PacketBasis {
        PacketBasis {
            dst_ip: self.src_addr.ip.clone(),
            prot_num: 6,
            msg: serialize_tcp(tpack)
        }
    }
}