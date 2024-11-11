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
    read_buf: Arc<Mutex<CircularBuffer<65536, u8>>>,
    write_buf: Arc<Mutex<SyncBuf<SendBuf>>>,
}

//TODO: deal with handshake timeouts
impl ConnectionSocket {
    pub fn new(
        state: Arc<RwLock<TcpState>>,
        src_addr: TcpAddress,
        dst_addr: TcpAddress,
        ip_sender: Arc<Sender<PacketBasis>>,
        ack_num: u32
    ) -> ConnectionSocket {
        let mut rand_rng = rand::thread_rng();
        let seq_num = rand_rng.gen::<u32>() / 2;
        ConnectionSocket {
            state,
            src_addr,
            dst_addr,
            seq_num,
            ip_sender,
            ack_num,
            win_size: 5000,
            read_buf: Arc::new(Mutex::new(CircularBuffer::<65536, u8>::new())),
            write_buf: Arc::new(Mutex::new(SyncBuf::new(SendBuf::new(5000)))),
        }
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
                _ => panic!("State not implemented!"),
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
            return TcpState::Established;
        }
        TcpState::SynSent
    }
    fn process_ack(&self, tpack: TcpPacket) -> TcpState {
        if has_only_flags(&tpack.header, ACK) {
            println!("Received final acknowledgement, TCP handshake successful!");
            return TcpState::Established;
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

    fn build_and_send(
        &mut self,
        payload: Vec<u8>,
        flags: u8
    ) -> result::Result<(), SendError<PacketBasis>> {
        let payload_len = payload.len();
        let new_pack = self.build_packet(payload, flags);
        let pbasis = self.packet_basis(new_pack);
        let increment_seq = if payload_len == 0 {
            if (flags & SYN) != 0 || (flags & FIN) != 0 { 1 } else { 0 }
        } else {
            payload_len
        };
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
        let checksum = tcp_header
            .calc_checksum_ipv4_raw(src_ip, dst_ip, payload.as_slice())
            .expect("Checksum calculation failed");
        tcp_header.checksum = checksum;
        return TcpPacket { header: tcp_header, payload };
    }
    /// Takes in a TCP header and a u8 representing flags and sets the corresponding flags in the header.
    fn set_flags(head: &mut TcpHeader, flags: u8) -> () {
        if (flags & SYN) != 0 {
            head.syn = true;
        }
        if (flags & ACK) != 0 {
            head.ack = true;
        }
        if (flags & FIN) != 0 {
            head.fin = true;
        }
        if (flags & RST) != 0 {
            head.rst = true;
        }
        if (flags & PSH) != 0 {
            head.psh = true;
        }
        if (flags & URG) != 0 {
            head.urg = true;
        }
    }
    /// Takes in a TCP packet and outputs a Packet Basis for its IP packet
    fn packet_basis(&self, tpack: TcpPacket) -> PacketBasis {
        PacketBasis {
            dst_ip: self.dst_addr.ip.clone(),
            prot_num: 6,
            msg: serialize_tcp(tpack),
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
        while !to_send.is_empty() {
            //Wait till send_onwards says you can access slf now
            let slf = slf.lock().unwrap();

            let writer = slf.write_buf.lock().unwrap();
            let mut write_buf = writer.buf.lock().unwrap();
            to_send = write_buf.fill_with(to_send);

            // Notify send_onwards of buffer update
            buf_update.notify_one();
        }
        //  slf.build_and_send(Vec::new(), ACK);
        0
    }

    fn send_onwards(slf: Arc<Mutex<Self>>, buf_update: Arc<Condvar>) -> () {
        loop {
            // Acquire a lock on `slf`
            let mut slf = slf.lock().unwrap();

            // Retrieve the next chunk of data to send
            let to_send = {
                let writer = slf.write_buf.lock().unwrap();
                let mut write_buf = writer.buf.lock().unwrap();
                // Wait for a buffer update notification from send
                write_buf = buf_update.wait(write_buf).unwrap();
                write_buf.next_data()
            };

            // Send data over the network
            if let Err(e) = slf.build_and_send(to_send, 0) {
                eprintln!("Error sending data: {}", e);
                break; // Exit the loop if there's an error
            }
        }
    }
}

//
//SEND AND RECV BUFFERS
//

const MAX_MSG_SIZE: usize = 1500;
const BUFFER_CAPACITY: usize = 65536;

#[derive(Debug)]
struct SyncBuf<T: TcpBuffer> {
    ready: Condvar,
    buf: Mutex<T>,
}

impl<T: TcpBuffer> SyncBuf<T> {
    fn new(buf: T) -> SyncBuf<T> {
        SyncBuf { ready: Condvar::new(), buf: Mutex::new(buf) }
    }
    fn alert_ready(&self) {
        self.ready.notify_one();
    }
    fn wait(&self) -> std::sync::MutexGuard<T> {
        let mut buf = self.buf.lock().unwrap();
        while !buf.ready() {
            buf = self.ready.wait(buf).unwrap();
        }
        buf
    }
}

#[derive(Debug)]
struct SendBuf {
    circ_buffer: CircularBuffer<BUFFER_CAPACITY, u8>,
    //una: usize, Don't need, b/c una will always be 0 technically
    nxt: usize,
    //lbw: usize Don't need b/c lbw will always be circ_buffer.len() technically
    rem_window: u16,
    num_acked: u32,
}

impl TcpBuffer for SendBuf {
    //Ready when buffer is not full
    fn ready(&self) -> bool {
        self.circ_buffer.len() != self.circ_buffer.capacity()
    }
}

trait TcpBuffer {
    fn ready(&self) -> bool;
}

impl SendBuf {
    fn new(window_size: u16) -> SendBuf {
        SendBuf {
            circ_buffer: CircularBuffer::new(),
            nxt: 0,
            rem_window: window_size,
            num_acked: 0,
        }
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
        let constraints = vec![
            self.rem_window as usize,
            self.circ_buffer.len() - self.nxt,
            MAX_MSG_SIZE
        ];
        let greatest_constraint = constraints.iter().min().unwrap();
        let upper_bound = self.nxt + greatest_constraint;
        let data = self.circ_buffer
            .range(self.nxt..upper_bound)
            .cloned()
            .collect();
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
    fn update_window(&mut self, new_window: u16) {
        self.rem_window = new_window;
    }
    fn is_full(&self) -> bool {
        self.circ_buffer.len() - self.circ_buffer.capacity() == 0
    }
}

#[derive(Debug)]
struct RecvBuf {
    circ_buffer: CircularBuffer<BUFFER_CAPACITY, u8>,
    //lbr: usize Don't need, lbr will always be 0
    //nxt: usize Don't need, nxt will always be circ_buffer.len()
    early_arrivals: HashMap<u32, Vec<u8>>,
    bytes_read: u32,
}

impl TcpBuffer for RecvBuf {
    fn ready(&self) -> bool {
        self.circ_buffer.len() != 0
    }
}

impl RecvBuf {
    pub fn new() -> RecvBuf {
        RecvBuf {
            circ_buffer: CircularBuffer::new(),
            early_arrivals: HashMap::new(),
            bytes_read: 0,
        }
    }

    ///Returns a vector of in-order data drained from the circular buffer, containing a number of elements equal to the specified amount
    ///or to the total amount of in-order data ready to go in the buffer
    pub fn read(&mut self, bytes: u16) -> Vec<u8> {
        let constraints = vec![bytes as usize, self.circ_buffer.len()];
        let greatest_constraint = constraints.iter().min().unwrap();
        self.circ_buffer.drain(..greatest_constraint).collect()
    }

    ///Adds the input data segment to the buffer if its sequence number is the next expected one. If not, inserts the segment into the
    ///early arrival hashmap. If data is ever added to the buffer, the early arrivals hashmap is checked to see if it contains the
    ///following expected segment, and the cycle continues until there are no more segments to add to the buffer
    ///Returns the next expected sequence number (the new ack number)
    pub fn add(&mut self, seq_num: u32, data: Vec<u8>) -> u32 {
        if seq_num == self.expected_seq() {
            self.circ_buffer.extend_from_slice(&data[..]);
            while let Some(next_data) = self.early_arrivals.remove(&self.expected_seq()) {
                self.circ_buffer.extend_from_slice(&next_data[..]);
            }
        } else {
            self.early_arrivals.insert(seq_num, data);
        }
        self.expected_seq()
    }
    ///Returns the next expected sequence number - only used privately, self.add() returns next sequence number too for public use
    fn expected_seq(&self) -> u32 {
        self.bytes_read + ((self.circ_buffer.len() + 1) as u32)
    }
    ///Returns the buffer's current window size
    pub fn window(&self) -> u16 {
        (self.circ_buffer.capacity() - self.circ_buffer.len()) as u16
    }
}
