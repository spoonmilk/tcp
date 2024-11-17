use crate::prelude::*;
use crate::tcp_utils::*;
use crate::utils::*;
use crate::send_recv_utils::*;

//TODO:
//Get post ZWP functionality to work better (put more inside send_onwards)
//Fix bug where we can't send packets that exceed remote window size


#[derive(Debug)]
pub struct ConnectionSocket {
    pub state: Arc<RwLock<TcpState>>,
    pub src_addr: TcpAddress,
    pub dst_addr: TcpAddress,
    ip_sender: Arc<Sender<PacketBasis>>,
    seq_num: u32, //Only edited in build_and_send() and viewed in build_packet()
    ack_num: u32, //Edited every time a packet is received (first_syn_ack(), process_syn_ack(), establish_handler()) and viewed in build_packet()
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
    ) -> ConnectionSocket {
        let mut rand_rng = rand::thread_rng();
        let seq_num = rand_rng.gen::<u32>() / 2;
        ConnectionSocket {
            state,
            src_addr,
            dst_addr,
            seq_num, 
            ip_sender,
            ack_num: 0, //We don't know what the ack number should be yet - in some sense, self.set_init_ack() finishes the initialization of the socket
            read_buf: Arc::new(SyncBuf::new(RecvBuf::new())),
            write_buf: Arc::new(SyncBuf::new(SendBuf::new(seq_num))),
        }
    }

    //Sending first messages in handshake
    pub fn first_syn(slf: Arc<Mutex<Self>>) {
        let mut slf = slf.lock().unwrap();
        slf.send_flags(SYN);
        let mut state = slf.state.write().unwrap();
        *state = TcpState::SynSent;
    }

    //
    //HANDLING INCOMING PACKETS
    //
    pub fn handle_packet(slf: Arc<Mutex<Self>>, tpack: TcpPacket) {
        //let slf_clone = Arc::clone(&slf); //Needed for zero window probing
        let mut slf = slf.lock().unwrap();
        //Universal packet reception actions
        if has_flags(&tpack.header, RST) { panic!("Received RST packet") } //Panics if RST flag received
        { //Update (remote) window size
            let mut write_buf = slf.write_buf.get_buf();
            write_buf.update_window(tpack.header.window_size);
        }
        //if tpack.header.window_size == 0 { Self::zero_window_probe(slf_clone) } //Sends probe packet if window is zero
        //State specific packet recpetion actions
        let new_state = {
            let slf_state = slf.state.read().unwrap();
            let state = slf_state.clone();
            drop(slf_state);
            match state {
                TcpState::Initialized => slf.process_syn(tpack),
                TcpState::SynSent => slf.process_syn_ack(tpack),
                TcpState::SynRecvd => slf.process_ack(tpack),
                TcpState::Established => slf.established_handle(tpack),
                _ => panic!("State not implemented!"),
            }
        };
        let mut state = slf.state.write().unwrap();
        *state = new_state;
    }
    fn process_syn(&mut self, tpack: TcpPacket) -> TcpState {
        if has_only_flags(&tpack.header, SYN) {
            //Deal with receiving first sequence number of TCP partner
            self.set_init_ack(tpack.header.sequence_number);
            //Send response (SYN + ACK in this case) and change state
            self.send_flags(SYN | ACK);
            return TcpState::SynRecvd;
        }
        panic!("Hmm, process_syn was called for a packet that was not SYN - check listener_recv()")
    }
    fn process_syn_ack(&mut self, tpack: TcpPacket) -> TcpState {
        if has_only_flags(&tpack.header, SYN | ACK) {
            //Deal with receiving first sequence number of TCP partner
            self.set_init_ack(tpack.header.sequence_number);
            //Send response (ACK in this case) and change state
            self.send_flags(ACK);
            return TcpState::Established;
        }
        TcpState::SynSent
    }
    fn process_ack(&self, tpack: TcpPacket) -> TcpState {
        if has_only_flags(&tpack.header, ACK) { return TcpState::Established; }
        TcpState::SynRecvd
    }
    fn established_handle(&mut self, tpack: TcpPacket) -> TcpState {
        match header_flags(&tpack.header) {
            ACK if tpack.payload.len() == 0 => { //Received an acknowledgement of data sent
                let mut send_buf = self.write_buf.get_buf();
                send_buf.ack_data(tpack.header.acknowledgment_number);
                self.write_buf.alert_ready();
            }
            ACK => { //Received data
                //Add data to Recv Buffer
                let mut recv_buf = self.read_buf.get_buf();
                let new_ack = recv_buf.add(tpack.header.sequence_number, tpack.payload.clone());
                self.ack_num = new_ack;
                drop(recv_buf);
                //Let any waiting receive() thread know that data was just added to the receive buffer
                self.read_buf.alert_ready();
                //Send acknowledgement for received data
                self.send_flags(ACK);
                println!("Acknowledged received data");
            }
            FIN => {} //Other dude wants to close the connection
            _ => eprintln!(
                "I got no clue how to deal with a packet that has flags: {}",
                header_flags(&tpack.header)
            ),
        }
        TcpState::Established
    }

    //UTILITIES
    fn set_init_ack(&mut self, rem_seq_num: u32) {
        //Set ack_num
        self.ack_num = rem_seq_num.clone();
        self.ack_num += 1; //Increment to be next expected value of sequence number
        //Set Recv Buffer's initial remote sequence number
        let mut read_buf = self.read_buf.get_buf();
        read_buf.set_init_seq(rem_seq_num);
    }

    //
    //BUILDING AND SENDING PACKETS
    //

    fn send_flags(&mut self, flags: u8) {
        self.build_and_send(Vec::new(), flags).expect("Error sending flag packet to partner");
        if (flags & SYN) != 0 || (flags & FIN) != 0 { self.seq_num += 1 }
    }

    fn send_data(&mut self, data: Vec<u8>) {
        let data_length = data.len();
        self.build_and_send(data, ACK).expect("Error sending data packet to partner");
        self.seq_num += data_length as u32;
    }

    fn send_probe(&self, data: Vec<u8>) {
        self.build_and_send(data, ACK).expect("Error sending probe packe to partner")
        //Don't adjust sequence number for probes
    }

    fn build_and_send(
        &self,
        payload: Vec<u8>,
        flags: u8,
    ) -> result::Result<(), SendError<PacketBasis>> {
        let new_pack = self.build_packet(payload, flags);
        let pbasis = self.packet_basis(new_pack);
        self.ip_sender.send(pbasis)
    }
    fn build_packet(&self, payload: Vec<u8>, flags: u8) -> TcpPacket {
        let window_size = { self.read_buf.get_buf().window() };
        let mut tcp_header = TcpHeader::new(
            self.src_addr.port,
            self.dst_addr.port,
            self.seq_num,
            window_size
        );
        tcp_header.acknowledgment_number = self.ack_num;
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

    //
    //SENDING AND RECVING
    //

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
        let write_buf = {
            let slf = slf.lock().unwrap();
            Arc::clone(&slf.write_buf)
        };
        let mut bytes_sent = 0;
        while !to_send.is_empty() {
            //Wait till send_onwards says you can access slf now
            let mut writer = write_buf.wait();
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

    fn send_onwards(slf: Arc<Mutex<Self>>, buf_update: Arc<Condvar>, terminate_send: Arc<AtomicBool>) -> () {
        let write_buf = {
            let slf = slf.lock().unwrap();
            Arc::clone(&slf.write_buf)
        };
        let mut snd_state = SendState::Sending;
        loop { 
            let to_send = match snd_state {
                SendState::Sending => {//Sending like usual
                    let mut writer = write_buf.get_buf();
                    let nxt_data = writer.next_data();
                    if let NextData::ZeroWindow(_) = nxt_data { thread::sleep(Duration::from_millis(5000)) }
                    nxt_data
                }
                SendState::Probing => {//Remote window was empty last we know, so wait then send the probe packet
                    thread::sleep(Duration::from_millis(5000));
                    let mut writer = write_buf.get_buf();
                    let nxt_data = writer.next_data();
                    if let NextData::Data(_) | NextData::NoData  = nxt_data {
                        let mut slf = slf.lock().unwrap();
                        slf.seq_num += 1; //Account for probe packet now successfully transmitted
                    }
                    nxt_data
                }
                SendState::Waiting => {//Buffer was empty last time we checked, so we check to see if we're done sending, and if not, wait till we receive and update 
                    if terminate_send.load(Ordering::SeqCst) { break; }
                    let mut writer = write_buf.get_buf();
                    writer = buf_update.wait(writer).unwrap();
                    let nxt_data = writer.next_data();
                    if let NextData::ZeroWindow(_) = nxt_data { thread::sleep(Duration::from_millis(5000)) }
                    nxt_data
                }
            };
            match to_send {
                NextData::Data(data) => {
                    let mut slf = slf.lock().unwrap();
                    slf.send_data(data);
                    snd_state = SendState::Sending
                }
                NextData::ZeroWindow(probe) => {
                    //thread::sleep(Duration::from_millis(5000));
                    let slf = slf.lock().unwrap();
                    slf.send_probe(probe);
                    snd_state = SendState::Probing
                }
                NextData::NoData => snd_state = SendState::Waiting,
            }
        }
    }

    fn run_timer(slf: Arc<Mutex<Self>>) -> () {
        let mut retr_timer = RetransmissionTimer::new();
        retr_timer.start_timer(); 
        loop {
            thread::sleep(retr_timer.rto); 
            if retr_timer.is_expired() {
                break; // Fail condition
            }
            retr_timer.do_retransmission();
            // Logic for actually doing the retransmission
        }
    }
    // TODO: Implement retransmission RTT calculation

    pub fn receive(slf: Arc<Mutex<Self>>, bytes: u16) -> Vec<u8> {
        let read_buf = {
            let slf = slf.lock().unwrap();
            Arc::clone(&slf.read_buf)
        };
        let mut recv_buf: std::sync::MutexGuard<'_, RecvBuf> = read_buf.wait();
        recv_buf.read(bytes)
    }
}

enum SendState {
    Sending,
    Probing, 
    Waiting
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

            // Since duration doesn't support abs, we finagle it a bit
            let delta = if measured_rtt > srtt {
                measured_rtt - srtt
            } else {
                srtt - measured_rtt
            };
            self.rttvar = Some(rttvar * 3 / 4 + delta / 4);
            self.srtt = Some(srtt * 7 / 8 + measured_rtt / 8);

            // Update RTO with SRTT and RTTVAR ; More absolute val finagling
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

