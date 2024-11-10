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
    writer: Arc<Mutex<Snd>>
}

//TODO: deal with handshake timeouts

impl ConnectionSocket {
    pub fn new(state: Arc<RwLock<TcpState>>, src_addr: TcpAddress, dst_addr: TcpAddress, ip_sender: Arc<Sender<PacketBasis>>, ack_num: u32) -> ConnectionSocket {
        let mut rand_rng = rand::thread_rng();
        let seq_num = rand_rng.gen::<u32>()/2;
        ConnectionSocket { state, src_addr, dst_addr, seq_num, ip_sender, ack_num, win_size: 5000, read_buf: Arc::new(Mutex::new(CircularBuffer::<65536, u8>::new())), writer: Arc::new(Mutex::new(Snd::new(5000))) }
    } 

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
 //SEND AND RECEIVE
    
    //Loops through sending packets of max size 1500 bytes until everything's been sent  
    pub fn send(slf: Arc<Mutex<Self>>, mut to_send: Vec<u8>) -> u16 {
        //Spawn send_onwards thread
        let buf_update = Arc::new(Condvar::new());
        let thread_buf_update = Arc::clone(&buf_update);
        let thread_slf = Arc::clone(&slf);
        thread::spawn(move || Self::send_onwards(thread_slf, thread_buf_update));
        //Continuously wait for there to be space in the buffer and add data till buffer is full
        while to_send.len() > 0 {
            //Wait till send_onwards says you can access slf now
            let mut slf = slf.lock().unwrap();
            slf = buf_update.wait(slf).unwrap();
            //Add data till write buffer is full
            let writer = slf.writer.lock().unwrap();
            let mut write_buf = writer.sbuf.lock().unwrap(); 
            to_send = write_buf.fill_with(to_send);
        }
       //  slf.build_and_send(Vec::new(), ACK);
        0
    }
    fn send_onwards(slf: Arc<Mutex<Self>>, buf_update: Arc<Condvar>) -> () {
        // loop {
        //     // Lock the connection socket
        //     let mut slf = slf.lock().unwrap();

        //     // Lock the writer and its send buffer
        //     let mut writer = slf.writer.lock().unwrap();
        //     let mut write_buf = writer.sbuf.lock().unwrap();

        //     // Get the next data to send
        //     let to_send = write_buf.next_data();
        //     // Check if there is data to send
        //     if !to_send.is_empty() {
        //         // Move the call to build_and_send inside the scope of the writer lock
        //         slf.build_and_send(to_send, 0).expect("Error sending to IpDaemon");
                
        //         // Notify that space is available in the buffer
        //         writer.alert_space_available();
        //         buf_update.notify_one();
        //     } else {
        //         // If there is no data to send, wait for space to become available
        //         drop(writer); // Drop the writer lock before waiting
        //         writer.wait_space_available();
        //     }
        // }
    }
    
}

const MAX_MSG_SIZE: usize = 1500;
const BUFFER_CAPACITY: usize = 65536;

#[derive(Debug)]
struct Snd {
    spc_available: Condvar,
    sbuf: Mutex<SendBuf>
}

impl Snd {
    fn new(window_size: u16) -> Snd {
        Snd { spc_available: Condvar::new(), sbuf: Mutex::new(SendBuf::new(window_size)) }
    }
    // Are these just unused b.c. we're passing in condvars?
    fn alert_space_available(&mut self) { self.spc_available.notify_one(); }
    fn wait_space_available(&mut self) -> std::sync::MutexGuard<SendBuf> {
        let mut sbuf = self.sbuf.lock().unwrap();
        while sbuf.is_full() {
            sbuf = self.spc_available.wait(sbuf).unwrap();
        }
        sbuf
    }
}

#[derive(Debug)]
struct SendBuf {
    circ_buffer: CircularBuffer<BUFFER_CAPACITY, u8>,
    //una: usize, Don't need, b/c una will always be 0 technically
    nxt: usize,
    //lbw: usize Don't need b/c lbw will always be circ_buffer.len() technically
    rem_window: u16,
    num_acked: u32
}

impl SendBuf {
    fn new(window_size: u16) -> SendBuf {
        SendBuf { circ_buffer: CircularBuffer::<BUFFER_CAPACITY, u8>::new(), nxt: 0, rem_window: window_size, num_acked: 0 }
    }
    ///Fills up the circular buffer with the data in filler until the buffer is full, 
    ///then returns the original input filler vector drained of the values added to the circular buffer
    fn fill_with(&mut self, mut filler: Vec<u8>) -> Vec<u8> {
        let available_spc = self.circ_buffer.capacity() - self.nxt;
        let to_add = filler.drain(..available_spc).collect::<Vec<u8>>();
        self.circ_buffer.extend_from_slice(&to_add[..]);
        filler
    }
    ///Returns a vector of data to be put in the next TcpPacket to send, taking into account the input window size of the receiver
    ///This vector contains as many bytes as possible up to the maximum payload size (1500)
    fn next_data(&mut self) -> Vec<u8> {
        //Takes into account the three constraints on how much data can be sent in the next TcpPacket (window size, maximum message size, and amount of data in the buffer)
        //and finds the appropriate
        let constraints = vec![self.rem_window as usize, self.circ_buffer.len() - self.nxt, MAX_MSG_SIZE];
        let greatest_constraint = constraints.iter().min().unwrap().clone(); 
        let upper_bound = self.nxt + greatest_constraint;
        let data = self.circ_buffer.drain(self.nxt..upper_bound).collect();
        self.nxt = upper_bound;
        data
    }
    //Acknowledges (drops) all sent bytes up to the one indicated by most_recent_ack
    fn ack_data(&mut self, most_recent_ack: u32) {
        let relative_ack = most_recent_ack - self.num_acked - 1;
        self.circ_buffer.drain(..relative_ack as usize);
        self.num_acked += relative_ack;
    }
    ///Updates the SendBuf's internal tracker of how many more bytes can be sent before filling the reciever's window
    fn update_window(&mut self, new_window: u16) { self.rem_window = new_window }
    fn is_full(&self) -> bool { self.circ_buffer.capacity() - self.circ_buffer.len() == 0 }
}