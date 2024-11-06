use crate::prelude::*;
use crate::utils::*;
use crate::tcp_utils::*;

#[derive(Debug)]
pub struct ConnectionSocket {
    pub state: Arc<RwLock<TcpState>>,
    pub src_addr: TcpAddress,
    pub dst_addr: TcpAddress,
    ip_sender: Arc<Sender<PacketBasis>>,
    seq_num: u32,
    ack_num: u32,
    win_size: u16
}
/* 
impl ConnectionSocket {
    pub fn handle_packet(slf: Arc<Mutex<Self>>, tpack: TcpPacket) -> () {
        //Process packet via typical state machine functioning (looks a lot like run does)
    }
    pub fn recv(slf: Arc<Mutex<Self>>, bytes: u16) -> (u16, Vec<u8>) {
        //Pulls up to bytes data out of recv buffer and returns amount of bytes read plus the data as a string
    }
    pub fn send(slf: Arc<Mutex<Self>>, to_send: Vec<u8>) -> () {
        //Loops through sending packets of max size 1500 bytes until everything's been sent 
    }
}
*/

impl ConnectionSocket {
    pub fn new(state: Arc<RwLock<TcpState>>, src_addr: TcpAddress, dst_addr: TcpAddress, ip_sender: Arc<Sender<PacketBasis>>) -> ConnectionSocket {
        let mut rand_rng = rand::thread_rng();
        let seq_num = rand_rng.gen::<u32>()/2;
        ConnectionSocket { state, src_addr, dst_addr, seq_num, ip_sender, ack_num: 0, win_size: 5000 }
    } 
    pub fn handle_packet(slf: Arc<Mutex<Self>>, tpack: TcpPacket) {
        let slf = slf.lock().unwrap(); 
        let new_state = {
            let slf_state = slf.state.write().unwrap();
                match *slf_state {
                TcpState::SynSent => slf.process_syn_ack(tpack),
                TcpState::SynRecvd => slf.process_ack(tpack), 
                TcpState::Established => slf.established_handle(tpack),
                _ => panic!("State not implemented!")
            }
        };
        let mut state = slf.state.write().unwrap();
        *state = new_state;
    }
    fn process_syn_ack(&self, tpack: TcpPacket) -> TcpState {
        if has_only_flags(&tpack.header, SYN | ACK) {
            let new_pack = self.build_packet(Vec::new(), ACK);
            let pbasis = self.packet_basis(new_pack);
            self.ip_sender.send(pbasis).expect("Error sending TCP packet to IP Daemon");
            return TcpState::Established
        }
        TcpState::SynSent
    }
    fn process_ack(&self, tpack: TcpPacket) -> TcpState {
        if has_only_flags(&tpack.header, ACK) {
            println!("Received final acknowledgement, TCP handshake successful!");
            let mut slf_state = self.state.write().unwrap();
            *slf_state = TcpState::Established;
            return TcpState::Established
        }
        TcpState::SynRecvd
    } 
    fn established_handle(&self, _tpack: TcpPacket) -> TcpState {
        println!("I got a packet wee!!!");
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

// !! DANGER !! YOU ARE ENTERING THE LEGACY CODE ZONE

// /// Runs an initialized TCP connection socket
    // pub fn run(self, sockman_recver: Receiver<SocketCmd>) -> () {
    //     loop {
    //         let command = sockman_recver.recv().expect("Error receiving from Socket Manager");
    //         let new_state = {
    //             let state = self.state.read().unwrap();
    //             match *state {
    //                 TcpState::Listening => panic!("I'm a freaking connection socket, why would I ever be in the listening state?!?"),
    //                 TcpState::SynSent => self.syn_sent_handle(command),
    //                 TcpState::SynRecvd => self.syn_recved_handle(command),
    //                 TcpState::Established => self.established_handle(command),
    //                 _ => panic!("I don't know what to do in these states yet!")
    //             }
    //         };
    //         let mut state = self.state.write().unwrap();
    //         *state = new_state;
    //     }
    // }