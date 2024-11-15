use crate::prelude::*;
use crate::tcp_utils::*;
use crate::utils::*;
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32};
use std::sync::Condvar;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct ConnectionSocket {
    pub state: Arc<RwLock<TcpState>>,
    pub src_addr: TcpAddress,
    pub dst_addr: TcpAddress,
    ip_sender: Arc<Sender<PacketBasis>>,
    seq_num: u32,             //Dynamic
    ack_num: Arc<AtomicU32>,  //Dynamic
    win_size: Arc<AtomicU16>, //Dynamic
    read_buf: Arc<SyncBuf<RecvBuf>>,
    write_buf: Arc<SyncBuf<SendBuf>>,
}

//TODO: deal with handshake timeouts
impl ConnectionSocket {
    pub fn new(
        state: Arc<RwLock<TcpState>>,
        src_addr: TcpAddress,
        dst_addr: TcpAddress,
        ip_sender: Arc<Sender<PacketBasis>>,
        ack_num: u32,
    ) -> ConnectionSocket {
        let mut rand_rng = rand::thread_rng();
        let seq_num = rand_rng.gen::<u32>() / 2;
        ConnectionSocket {
            state,
            src_addr,
            dst_addr,
            seq_num,
            ip_sender,
            ack_num: Arc::new(AtomicU32::new(ack_num)),
            win_size: Arc::new(AtomicU16::new(BUFFER_CAPACITY as u16)),
            read_buf: Arc::new(SyncBuf::new(RecvBuf::new())),
            write_buf: Arc::new(SyncBuf::new(SendBuf::new(seq_num))),
        }
    }

    /*
    pub fn recv(slf: Arc<Mutex<Self>>, bytes: u16) -> (u16, Vec<u8>) {
        //Pulls up to bytes data out of recv buffer and returns amount of bytes read plus the data as a string
    }*/

    //Sending first messages in handshake

    pub fn first_syn(slf: Arc<Mutex<Self>>) {
        let mut slf = slf.lock().unwrap();
        slf.build_and_send(Vec::new(), SYN)
            .expect("Error sending to IpDaemon");
        let mut state = slf.state.write().unwrap();
        *state = TcpState::SynSent;
    }
    pub fn first_syn_ack(slf: Arc<Mutex<Self>>, t_pack: TcpPacket) {
        let mut slf = slf.lock().unwrap();

        slf.ack_num.fetch_add(1, Ordering::SeqCst);
        slf.build_and_send(Vec::new(), SYN | ACK)
            .expect("Error sending to IpDaemon");
        let mut state = slf.state.write().unwrap();

        // Initializing send and receive buffers
        let mut send_buf = slf.write_buf.get_buf();
        send_buf.update_window(t_pack.header.window_size);
        println!("Remote window has size: {}", t_pack.header.window_size);
        let mut recv_buf = slf.read_buf.get_buf();
        recv_buf.set_seq(t_pack.header.sequence_number);

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
            self.ack_num
                .store(tpack.header.sequence_number, Ordering::SeqCst);
            self.ack_num.fetch_add(1, Ordering::SeqCst); //A SYN was received
            self.build_and_send(Vec::new(), ACK)
                .expect("Error sending TCP packet to IP Daemon");
            let mut send_buf = self.write_buf.get_buf();
            send_buf.update_window(tpack.header.window_size);
            println!("Remote window has size: {}", tpack.header.window_size);
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
        println!("Got a packet");
        match header_flags(&tpack.header) {
            ACK if tpack.payload.len() == 0 => {
                //Received an acknowledgement of data sent
                let mut send_buf = self.write_buf.get_buf();
                send_buf.ack_data(tpack.header.acknowledgment_number);
            }
            ACK => {
                //Received data
                let mut recv_buf = self.read_buf.get_buf();
                self.ack_num.store(
                    recv_buf.add(tpack.header.sequence_number, tpack.payload.clone()),
                    Ordering::SeqCst,
                );
                self.win_size.store(recv_buf.window(), Ordering::SeqCst);
                println!("new window size: {}", self.win_size.load(Ordering::SeqCst));
                let mut send_buf = self.write_buf.get_buf();
                send_buf.update_window(recv_buf.window());
                drop(recv_buf);
                drop(send_buf);
                match self.build_and_send(Vec::new(), ACK) {
                    Ok(_) => println!("Acknowledged received data"),
                    Err(e) => eprintln!("Error sending ACK: {}", e),
                }
                self.read_buf.alert_ready();
            }
            FIN => {} //Other dude wants to close the connection
            _ => eprintln!(
                "I got no clue how to deal with a packet that has flags: {}",
                header_flags(&tpack.header)
            ),
        }
        TcpState::Established
    }

    //
    //BUILDING AND SENDING PACKETS
    //

    fn build_and_send(
        &mut self,
        payload: Vec<u8>,
        flags: u8,
    ) -> result::Result<(), SendError<PacketBasis>> {
        let payload_len = payload.len();
        let new_pack = self.build_packet(payload, flags);
        let pbasis = self.packet_basis(new_pack);
        let increment_seq = if payload_len == 0 {
            if (flags & SYN) != 0 || (flags & FIN) != 0 {
                1
            } else {
                0
            }
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
            self.seq_num,
            self.win_size.load(Ordering::SeqCst),
        );
        tcp_header.acknowledgment_number = self.ack_num.load(Ordering::SeqCst);
        ConnectionSocket::set_flags(&mut tcp_header, flags);
        let src_ip = self.src_addr.ip.clone().octets();
        let dst_ip = self.dst_addr.ip.clone().octets();
        let checksum = tcp_header
            .calc_checksum_ipv4_raw(src_ip, dst_ip, payload.as_slice())
            .expect("Checksum calculation failed");
        tcp_header.checksum = checksum;
        return TcpPacket {
            header: tcp_header,
            payload,
        };
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

    // Loops through sending packets of max size 1500 bytes until everything's been sent
    pub fn send(slf: Arc<Mutex<Self>>, mut to_send: Vec<u8>) -> u16 {
        // Condvar for checking if the buffer has been updated
        let buf_update = Arc::new(Condvar::new());
        let thread_buf_update = Arc::clone(&buf_update);
        // AtomicBool for checking if the send should stop
        let terminate_send = Arc::new(AtomicBool::new(false));
        let thread_terminate_send = Arc::clone(&terminate_send);

        let thread_slf = Arc::clone(&slf);

        let thread_send_onwards = thread::Builder::new()
            .name("send_onwards".to_string())
            .spawn(move || Self::send_onwards(thread_slf, thread_buf_update, thread_terminate_send))
            .expect("Could not spawn thread");

        //Continuously wait for there to be space in the buffer and add data till buffer is full
        let mut bytes_sent = 0;
        while !to_send.is_empty() {
            //Wait till send_onwards says you can access slf now
            let slf = slf.lock().unwrap();
            let mut writer = slf.write_buf.wait();
            let old_len = to_send.len();
            to_send = writer.fill_with(to_send);
            bytes_sent += old_len - to_send.len();
            // Notify send_onwards of buffer update
            buf_update.notify_one();
        }
        terminate_send.store(true, Ordering::SeqCst);

        thread_send_onwards
            .join()
            .expect("Send onwards thread panicked");
        bytes_sent as u16
    }

    fn send_onwards(
        slf: Arc<Mutex<Self>>,
        buf_update: Arc<Condvar>,
        terminate_send: Arc<AtomicBool>,
    ) -> () {
        let write_buf = {
            let slf = slf.lock().unwrap();
            Arc::clone(&slf.write_buf)
        };
        loop {
            // Retrieve the next chunk of data to send
            let mut to_send: Vec<u8> = {
                let mut writer = write_buf.buf.lock().unwrap();
                writer.next_data()
            };
            if to_send.is_empty() {
                if terminate_send.load(Ordering::SeqCst) {
                    break;
                } else {
                    let mut writer = write_buf.buf.lock().unwrap();
                    writer = buf_update.wait(writer).unwrap();
                    to_send = writer.next_data();
                }
            }
            // Send data over the network
            let mut slf = slf.lock().unwrap();
            if let Err(e) = slf.build_and_send(to_send, ACK) {
                eprintln!("Error sending data: {}", e);
                break; // Exit the loop if there's an error
            }
        }
    }

    pub fn receive(slf: Arc<Mutex<Self>>, bytes: u16) -> Vec<u8> {
        let mut received = Vec::new();
        let read_buf = {
            let slf = slf.lock().unwrap();
            Arc::clone(&slf.read_buf)
        };
        loop {
            let remaining_amt = bytes - (received.len() as u16);
            if remaining_amt == 0 {
                // Upon receiving all data, respond with an acknowledgment
                break received;
            }
            let mut recv_buf = read_buf.wait();
            let just_received = recv_buf.read(remaining_amt);
            received.extend(just_received);
        }
    }
}

//
//SEND AND RECV BUFFERS
//

const MAX_MSG_SIZE: usize = 1480;
const BUFFER_CAPACITY: usize = 65535;

#[derive(Debug)]
struct SyncBuf<T: TcpBuffer> {
    ready: Condvar,
    buf: Mutex<T>,
}

impl<T: TcpBuffer> SyncBuf<T> {
    pub fn new(buf: T) -> SyncBuf<T> {
        SyncBuf {
            ready: Condvar::new(),
            buf: Mutex::new(buf),
        }
    }
    pub fn alert_ready(&self) {
        self.ready.notify_one();
    }
    pub fn wait(&self) -> std::sync::MutexGuard<T> {
        let mut buf = self.buf.lock().unwrap();
        while !buf.ready() {
            buf = self.ready.wait(buf).unwrap();
        }
        buf
    }
    pub fn get_buf(&self) -> std::sync::MutexGuard<T> {
        self.buf.lock().unwrap()
    }
}

trait TcpBuffer {
    fn ready(&self) -> bool;
}

#[derive(Debug)]
struct SendBuf {
    circ_buffer: CircularBuffer<BUFFER_CAPACITY, u8>,
    //una: usize, Don't need, b/c una will always be 0 technically
    nxt: usize, // Pointer to next byte to be sent ; NOTE, UPDATE AS BYTES DRAINED
    //lbw: usize Don't need b/c lbw will always be circ_buffer.len() technically
    rem_window: u16,
    num_acked: u32,
    our_init_seq: u32,
}

impl TcpBuffer for SendBuf {
    //Ready when buffer is not full
    fn ready(&self) -> bool {
        self.circ_buffer.len() != self.circ_buffer.capacity()
    }
}

impl SendBuf {
    fn new(our_init_seq: u32) -> SendBuf {
        SendBuf {
            circ_buffer: CircularBuffer::new(),
            nxt: 0,
            rem_window: 0,
            num_acked: 0,
            our_init_seq, //OUR
        }
    }
    ///Fills up the circular buffer with the data in filler until the buffer is full,
    ///then returns the original input filler vector drained of the values added to the circular buffer
    fn fill_with(&mut self, mut filler: Vec<u8>) -> Vec<u8> {
        let available_spc = self.circ_buffer.capacity() - self.circ_buffer.len();
        let to_add = filler
            .drain(..std::cmp::min(available_spc, filler.len()))
            .collect::<Vec<u8>>();

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
            MAX_MSG_SIZE,
        ];
        // Obtain greatest (unintuitively, smallest) constraint
        let greatest_constraint = constraints.iter().min().unwrap();
        let upper_bound = self.nxt + greatest_constraint;
        // Grab data, feed to sender
        let data: Vec<u8> = self
            .circ_buffer
            .range(self.nxt..upper_bound)
            .cloned()
            .collect();
        self.nxt = upper_bound;
        data
    }
    ///Acknowledges (drops) all sent bytes up to the one indicated by most_recent_ack
    fn ack_data(&mut self, most_recent_ack: u32) {
        // Caclulate relative acknowledged data
        let relative_ack = most_recent_ack - (self.num_acked + self.our_init_seq + 1);
        // Decrement nxt pointer to match dropped data ; compensation for absence of una
        self.nxt -= relative_ack as usize;
        // Drain out acknowledged data
        self.circ_buffer.drain(..relative_ack as usize);
        self.num_acked += relative_ack;
    }
    ///Updates the SendBuf's internal tracker of how many more bytes can be sent before filling the reciever's window
    fn update_window(&mut self, new_window: u16) {
        self.rem_window = new_window;
    }
}

#[derive(Debug)]
struct RecvBuf {
    circ_buffer: CircularBuffer<BUFFER_CAPACITY, u8>,
    //lbr: usize Don't need, lbr will always be 0
    //nxt: usize Don't need, nxt will always be circ_buffer.len()
    early_arrivals: HashMap<u32, Vec<u8>>,
    bytes_read: u32,
    rem_init_seq: u32,
}

impl TcpBuffer for RecvBuf {
    //Ready when buffer has some elements
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
            rem_init_seq: 0, //We don't know yet *shrug*
        }
    }

    ///Returns a vector of in-order data drained from the circular buffer, containing a number of elements equal to the specified amount
    ///or to the total amount of in-order data ready to go in the buffer
    pub fn read(&mut self, bytes: u16) -> Vec<u8> {
        let constraints = vec![bytes as usize, self.circ_buffer.len()];
        let greatest_constraint = constraints.iter().min().unwrap();
        let data: Vec<u8> = self.circ_buffer.drain(..greatest_constraint).collect();
        self.bytes_read += data.len() as u32;
        data
    }

    ///Adds the input data segment to the buffer if its sequence number is the next expected one. If not, inserts the segment into the
    ///early arrival hashmap. If data is ever added to the buffer, the early arrivals hashmap is checked to see if it contains the
    ///following expected segment, and the cycle continues until there are no more segments to add to the buffer
    ///Returns the next expected sequence number (the new ack number)
    pub fn add(&mut self, seq_num: u32, data: Vec<u8>) -> u32 {
        println!(
            "sequence number: {}\nexpected sequence number: {}",
            seq_num,
            self.expected_seq()
        );
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
        self.bytes_read + ((self.circ_buffer.len() + 1) as u32) + self.rem_init_seq
    }
    ///Returns the buffer's current window size
    pub fn window(&self) -> u16 {
        (self.circ_buffer.capacity() - self.circ_buffer.len()) as u16
    }
    /// Set the sequence number
    pub fn set_seq(&mut self, seq_num: u32) {
        self.rem_init_seq = seq_num;
    }
}
/* Algorithm for calculatating RTO and successive:

SEE RFC 6298
Note: G assumed to be 0

init state:
RTO_INITIAL = 1
min_rto, max_rto, alpha, beta = 1, 100, 0.125, 0.25
retransmission count = 0

AFTER FIRST RTT -> R
SRTT = R
RTTVAR = R/2
RTO = SRTT + (K * RTTVAR)

AFTER SECOND RTT -> R'
SRTT = (1 - ALPHA) * SRTT + ALPHA * R'
RTTVAR = (1 - BETA) * RTTVAR + BETA * ABS(SRTT - R')

SUCCESSIVE:
RTO = SRTT + (K * RTTVAR)

CONSTANTS:
- BETA = 1/4
- ALPHA = 1/8
- K = 4

*/
// CONSTANTS
const MIN_RTO: u64 = 1; // Milliseconds
const MAX_RTO: u64 = 100; // Milliseconds

pub struct RetransmissionTimer {
    rto: Duration,              // RTO: retransmission timeout
    srtt: Option<Duration>,     // Initially none, see above algo
    rttvar: Option<Duration>,   // Initially none, see above algo
    min_rto: Duration,          // Minimum RTO: 1ms for imp, 150-250ms for testing
    max_rto: Duration,          // Maximum RTO: 100ms(?)
    retransmission_count: u32,  // Attempt counter ; stop at 3
    time_since_resend: Option<Instant>, // Created on startup with val 0 then updated at each retransmission ; RTT equivalent
}
impl RetransmissionTimer {
    pub fn new() -> RetransmissionTimer {
        RetransmissionTimer {
            rto: Duration::from_millis(MIN_RTO),
            srtt: None,
            rttvar: None,
            min_rto: Duration::from_millis(MIN_RTO),
            max_rto: Duration::from_millis(MAX_RTO),
            retransmission_count: 0,
            time_since_resend: None,
        }
    }
    fn update_rto(&mut self, measured_rtt: Duration) {
        if let (Some(srtt), Some(rttvar)) = (self.srtt, self.rttvar) {
            // Successive rtt algorithms ; see RFC 6298 2.3

            // Since duration doesn't support abs, we fenagle it a bit
            let delta = if measured_rtt > srtt {
                measured_rtt - srtt
            } else {
                srtt - measured_rtt
            };
            self.rttvar = Some(rttvar * 3 / 4 + delta / 4);
            self.srtt = Some(srtt * 7 / 8 + measured_rtt / 8);

            // Update RTO with SRTT and RTTVAR
            self.rto = self.srtt.unwrap() + self.rttvar.unwrap() * 4;
            self.rto = self.rto.clamp(self.min_rto, self.max_rto);
        } else {
            // First rtt algorithm ; see RFC 6298 2.2
            self.srtt = Some(measured_rtt);
            self.rttvar = Some(measured_rtt / 2);
            self.rto = self.srtt.unwrap() + (4 * self.rttvar.unwrap());
        }
    }
    fn do_retransmission(&mut self) { 
        self.retransmission_count += 1;
        self.rto *= 2; // RFC 6298 (5.5) 
    }
    pub fn start_timer (&mut self) {
        self.time_since_resend = Some(Instant::now());
    }
    pub fn is_expired (&mut self) -> bool {
        if let Some(time_since_resend) = self.time_since_resend {
            time_since_resend.elapsed() > self.rto
        } else {
            false
        }
    }
    pub fn reset (&mut self) {
        self.retransmission_count = 0;
        self.rto = Duration::from_millis(MIN_RTO);
        self.rttvar = None;
        self.srtt = None;
        self.time_since_resend = None;
    }


}

