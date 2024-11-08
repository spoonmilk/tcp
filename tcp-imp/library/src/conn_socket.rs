use crate::prelude::*;
use crate::utils::*;
use crate::tcp_utils::*;
use std::sync::Condvar;

#[derive(Debug)]
pub struct ConnectionSocket {
    pub state: Arc<RwLock<TcpState>>,
    pub src_addr: TcpAddress,
    pub dst_addr: TcpAddress,
    ip_sender: Arc<Sender<PacketBasis>>,
    seq_num: u32,
    ack_num: u32,
    win_size: u16,
    read_buf: Arc<Mutex<CircularBuffer::<65536, u8>>>,
    write_buf: Arc<Mutex<CircularBuffer::<65536, u8>>>
}

//TODO: deal with handshake timeouts

impl ConnectionSocket {
    pub fn new(state: Arc<RwLock<TcpState>>, src_addr: TcpAddress, dst_addr: TcpAddress, ip_sender: Arc<Sender<PacketBasis>>, ack_num: u32) -> ConnectionSocket {
        let mut rand_rng = rand::thread_rng();
        let seq_num = rand_rng.gen::<u32>()/2;
        ConnectionSocket { state, src_addr, dst_addr, seq_num, ip_sender, ack_num, win_size: 5000, read_buf: Arc::new(Mutex::new(CircularBuffer::<65536, u8>::new())), write_buf: Arc::new(Mutex::new(CircularBuffer::<65536, u8>::new())) }
    } 

    //SEND AND RECEIVE
    
    //Loops through sending packets of max size 1500 bytes until everything's been sent 
    /* 
    pub fn send(slf: Arc<Mutex<Self>>, to_send: Vec<u8>) -> u16 {
        //Spawn send_onwards thread
        let buf_update = Arc::new(Condvar::new());
        let thread_slf = Arc::clone(&slf);
        let thread_buf_update = Arc::clone(&buf_update);
        thread::spawn(move || Self::send_onwards(thread_slf, thread_buf_update));
        //Continuously wait for there to be space in the buffer and add data till buffer is full
        while to_send.len() > 0 {
            //Wait till send_onwards says you can access slf now
            let mut slf = slf.lock().unwrap();
            slf = buf_update.wait(slf).unwrap();
            //Add data till write buffer is full
            let write_buf = slf.write
            let bytes_to_add = slf.write_buf.len()
        }
        //slf.build_and_send(Vec::new(), ACK);
        0
    }
    fn send_onwards(slf: Arc<Mutex<Self>>, buf_update: Arc<Condvar>) -> () {

    }*/
    /*
    pub fn recv(slf: Arc<Mutex<Self>>, bytes: u16) -> (u16, Vec<u8>) {
        //Pulls up to bytes data out of recv buffer and returns amount of bytes read plus the data as a string
    }*/

    //Sending first messages in handshake
    
    pub fn first_syn(slf: Arc<Mutex<Self>>) {
        let mut slf = slf.lock().unwrap();
        slf.build_and_send(Vec::new(), SYN).expect("Error sending to IpDaemon");
        let mut state = slf.state.write().unwrap();
        *state = TcpState::SynSent;
    }
    pub fn first_syn_ack(slf: Arc<Mutex<Self>>) {
        let mut slf = slf.lock().unwrap();
        slf.ack_num += 1; 
        slf.build_and_send(Vec::new(), SYN | ACK).expect("Error sending to IpDaemon");
        let mut state = slf.state.write().unwrap();
        *state = TcpState::SynRecvd;
    }

    //
    //HANDLING INCOMING PACKETS
    //

    pub fn handle_packet(slf: Arc<Mutex<Self>>, tpack: TcpPacket) {
        let mut slf = slf.lock().unwrap(); 
        let new_state = {
            let slf_state = slf.state.read().unwrap();
            let state = slf_state.clone();
            drop(slf_state);
            match state {
                TcpState::SynSent => slf.process_syn_ack(tpack),
                TcpState::SynRecvd => slf.process_ack(tpack), 
                TcpState::Established => slf.established_handle(tpack),
                _ => panic!("State not implemented!")
            }
        };
        let mut state = slf.state.write().unwrap();
        *state = new_state;
    }
    fn process_syn_ack(&mut self, tpack: TcpPacket) -> TcpState {
        if has_only_flags(&tpack.header, SYN | ACK) {
            self.ack_num = tpack.header.sequence_number;
            self.ack_num += 1; //A SYN was received
            self.build_and_send(Vec::new(), ACK).expect("Error sending TCP packet to IP Daemon");
            return TcpState::Established
        }
        TcpState::SynSent
    }
    fn process_ack(&self, tpack: TcpPacket) ->TcpState {
        if has_only_flags(&tpack.header, ACK) {
            println!("Received final acknowledgement, TCP handshake successful!");
            return TcpState::Established
        }
        TcpState::SynRecvd
    } 
    fn established_handle(&mut self, tpack: TcpPacket) -> TcpState {
        self.ack_num += tpack.payload.len() as u32;
        println!("I got a packet wee!!!");
        TcpState::Established
    }

    //
    //BUILDING AND SENDING PACKETS
    //

    fn build_and_send(&mut self, payload: Vec<u8>, flags: u8) -> result::Result<(), SendError<PacketBasis>> {
        let payload_len = payload.len();
        let new_pack = self.build_packet(payload, flags);
        let pbasis = self.packet_basis(new_pack);
        let increment_seq = if payload_len == 0 {
            if (flags & SYN) != 0 || (flags & FIN) != 0 { 1 } else { 0 }
        } else { payload_len };
        self.seq_num += increment_seq as u32;
        self.ip_sender.send(pbasis)
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
        } 
        if flags & ACK != 0 {
            head.ack = true;
        }
        if flags & FIN != 0 {
            head.fin = true;
        }
        if flags & RST != 0 {
            head.rst = true;
        }
        if flags & PSH != 0 {
            head.psh = true;
        } 
        if flags & URG != 0 {
            head.urg = true;
        }
    } 
    /// Takes in a TCP packet and outputs a Packet Basis for its IP packet
    fn packet_basis(&self, tpack: TcpPacket) -> PacketBasis {
        PacketBasis {
            dst_ip: self.dst_addr.ip.clone(),
            prot_num: 6,
            msg: serialize_tcp(tpack)
        }
    }
}