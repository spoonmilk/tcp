use crate::prelude::*;
use crate::retransmission::*;
use crate::send_recv_utils::*;
use crate::tcp_utils::*;
use crate::utils::*;
//TODO:
//Get post ZWP functionality to work better (put more inside send_onwards)
type SocketId = u16;
const ZWP_TIMEOUT: u64 = 5000; //in millis

#[derive(Debug)]
pub struct ConnectionSocket {
    pub state: Arc<RwLock<TcpState>>,
    pub src_addr: TcpAddress,
    pub dst_addr: TcpAddress,
    sid: SocketId,
    closed_sender: Arc<Sender<SocketId>>,
    ip_sender: Arc<Sender<PacketBasis>>,
    stop_probing_recver: Arc<Mutex<Receiver<()>>>, //Needs to be an Arc so that it can be cloned and self can be dropped, needs to be a mutex so Rust doesn't freak out about two threads using the receiver at once
    seq_num: u32, //Only edited in build_and_send() and viewed in build_packet()
    ack_num: u32, //Edited every time a packet is received (first_syn_ack(), process_syn_ack(), establish_handler()) and viewed in build_packet()
    read_buf: Arc<SyncBuf<RecvBuf>>,
    write_buf: Arc<SyncBuf<SendBuf>>,
    retr_timer: Arc<Mutex<RetransmissionTimer>>,
    // retr_queue: Arc<Mutex<RetransmissionQueue>>,
}

impl ConnectionSocket {
    pub fn new(
        state: Arc<RwLock<TcpState>>,
        src_addr: TcpAddress,
        dst_addr: TcpAddress,
        closed_sender: Arc<Sender<SocketId>>,
        ip_sender: Arc<Sender<PacketBasis>>,
    ) -> ConnectionSocket {
        let mut rand_rng = rand::thread_rng();
        let seq_num = rand_rng.gen::<u32>() / 2;
        let (stop_probing_sender, stop_probing_recver) = channel::<()>();
        ConnectionSocket {
            state,
            src_addr,
            dst_addr,
            seq_num,
            sid: 0, //We don't know on intialization - we'll only know once we get added to the socket table (and self.set_sid() is called)
            closed_sender,
            ip_sender,
            stop_probing_recver: Arc::new(Mutex::new(stop_probing_recver)),
            ack_num: 0, //We don't know what the ack number should be yet - in some sense, self.set_init_ack() finishes the initialization of the socket
            read_buf: Arc::new(SyncBuf::new(RecvBuf::new())),
            write_buf: Arc::new(SyncBuf::new(SendBuf::new(seq_num, stop_probing_sender))),
            retr_timer: Arc::new(Mutex::new(RetransmissionTimer::new())),
            // retr_queue: Arc::new(Mutex::new(RetransmissionQueue::new())),
        }
    }
    /// Periodically does checking/elimination/retransmission from the queue and timer
    pub fn time_check(slf: Arc<Mutex<Self>>) {
        let rto = {
            let slf = slf.lock().unwrap();
            let retr_timer = slf.retr_timer.lock().unwrap();
            retr_timer.rto
        };
        loop {
            thread::sleep(rto);
            {
                let mut slf = slf.lock().unwrap();
                let current_rto = slf.retr_timer.lock().unwrap().rto;
                let retr_segs = {
                    let write_buf = Arc::clone(&slf.write_buf);
                    let mut writer = write_buf.get_buf();
                    writer.check_timeouts(current_rto)
                };
                for seg in retr_segs {
                    slf.send_segment(seg.seq_num, seg.payload.clone(), seg.flags, seg.checksum);
                    {
                        let mut retr_timer = slf.retr_timer.lock().unwrap();
                        retr_timer.do_retransmission();
                        println!("Current rto: {}", retr_timer.rto.as_millis());
                    }
                }
            }
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
    //TODO: Clean this ugly ass function up
    pub fn handle_packet(slf: Arc<Mutex<Self>>, tpack: TcpPacket, ip_head: Ipv4Header) {
        //let slf_clone = Arc::clone(&slf); //Needed for zero window probing
        let mut slf = slf.lock().unwrap();
        //Universal packet reception actions
        if has_flags(&tpack.header, RST) {
            panic!("Received RST packet");
        } //Panics if RST flag received
          //TODO: Get rid of clone, but I'm tired and lazy - will fix later - Alex
        if !slf.check_tcp_checksum(tpack.clone(), ip_head) {
            eprintln!("Received packet with bad checksum, dropping.");
            return;
        }
        {
            //Update (remote) window size
            let mut write_buf = slf.write_buf.get_buf();
            write_buf.update_window(tpack.header.window_size);
        }
        //State specific packet reception actions
        let new_state = {
            let slf_state = slf.state.read().unwrap();
            let state = slf_state.clone();
            drop(slf_state);
            match state {
                TcpState::Listening | TcpState::AwaitingRun => {
                    panic!("Connection socket should not be in state {state:?}")
                }
                TcpState::Initialized => slf.process_syn(tpack),
                TcpState::SynSent => slf.process_syn_ack(tpack),
                TcpState::SynRecvd => slf.process_ack(tpack),
                TcpState::Established => slf.established_handle(tpack),
                TcpState::FinWait1 => slf.fin_wait_1_handler(tpack),
                TcpState::FinWait2 => slf.fin_wait_2_handler(tpack),
                TcpState::TimeWait => slf.time_wait_handler(tpack),
                TcpState::CloseWait => slf.close_wait_handler(tpack),
                TcpState::LastAck => slf.last_ack_handler(tpack),
                TcpState::Closed => slf.closed_handler(tpack),
            }
        };
        let mut state = slf.state.write().unwrap();
        *state = new_state;
    }
    /// Checks if a tcp packet complies to the TCP protocol checksum
    fn check_tcp_checksum(&mut self, tpack: TcpPacket, ip_head: Ipv4Header) -> bool {
        let proper_checksum = {
            match tpack.header.calc_checksum_ipv4(&ip_head, &tpack.payload) {
                Ok(checksum) => checksum,
                Err(_) => panic!("Error in checksum calculation"),
            }
        };
        proper_checksum == tpack.header.checksum
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
    //NOTE: For William: Should new_ack do anything here? if not, you can just remove everything
    //except ack_rt :)
    fn process_syn_ack(&mut self, tpack: TcpPacket) -> TcpState {
        println!("Processing a syn | ack for my ack");
        if has_only_flags(&tpack.header, SYN | ACK) {
            //Deal with receiving first sequence number of TCP partner
            self.set_init_ack(tpack.header.sequence_number);
            {
                {
                    let mut recv_buf = self.read_buf.get_buf();
                    let new_ack = recv_buf.add(tpack.header.sequence_number, tpack.payload.clone());
                }
                self.ack_rt(tpack.header.acknowledgment_number);
            }
            //Send response (ACK in this case) and change state
            self.send_flags(ACK);
            return TcpState::Established;
        }
        TcpState::SynSent
    }
    fn process_ack(&mut self, tpack: TcpPacket) -> TcpState {
        println!("Processing an ack for my syn | ack");
        if has_only_flags(&tpack.header, ACK) {
            {
                {
                    let mut recv_buf = self.read_buf.get_buf();
                    let new_ack = recv_buf.add(tpack.header.sequence_number, tpack.payload.clone());
                }
                self.ack_rt(tpack.header.acknowledgment_number);
            }
            return TcpState::Established;
        }
        TcpState::SynRecvd
    }
    fn established_handle(&mut self, tpack: TcpPacket) -> TcpState {
        match header_flags(&tpack.header) {
            ACK if tpack.payload.len() == 0 => self.ack(tpack),
            ACK => self.absorb_and_acknowledge(tpack),
            FIN => {
                //Other dude wants to close the connection
                // Okay! I will close!
                self.send_flags(ACK);
                return TcpState::CloseWait;
            }
            _ => eprintln!(
                "ESTABLISHED: I got no clue how to deal with a packet that has flags: {}",
                header_flags(&tpack.header)
            ),
        }
        TcpState::Established
    }
    fn fin_wait_1_handler(&mut self, tpack: TcpPacket) -> TcpState {
        //Still accepts both acks for data sent and incoming data
        //Transitions to FinWait2 upon reception of an ACK for its FIN
        match header_flags(&tpack.header) {
            ACK if tpack.payload.len() == 0 => {
                if tpack.header.acknowledgment_number == self.seq_num + 1 {
                    //Received packet is ack for FIN we sent
                    return TcpState::FinWait2;
                } else {
                    //Received packet is ack for a previous packet we sent
                    self.ack(tpack)
                }
            }
            ACK => self.absorb_and_acknowledge(tpack),
            _ => eprintln!(
                "FIN_WAIT_1: I shouldn't get packet with flags: {}",
                header_flags(&tpack.header)
            ),
        }
        TcpState::FinWait1
    }
    fn fin_wait_2_handler(&mut self, tpack: TcpPacket) -> TcpState {
        //Still accepts both acks for data sent and incoming data
        //Transitions to TimeWiat upon reception of a FIN
        match header_flags(&tpack.header) {
            ACK if tpack.payload.len() == 0 => self.ack(tpack),
            ACK => self.absorb_and_acknowledge(tpack),
            FIN => {
                //Other dude wants to finish this closing business
                self.send_flags(ACK);
                return TcpState::TimeWait;
            }
            _ => eprintln!(
                "FIN_WAIT_2: I shouldn't get packet with flags: {}",
                header_flags(&tpack.header)
            ),
        }
        TcpState::FinWait2
    }
    fn time_wait_handler(&mut self, tpack: TcpPacket) -> TcpState {
        //Still accepts acks for its own data, but doesn't expect any incoming data
        //Transitions to Closed after 2 * MAX RTO passes
        //TODO: needs to spawn a thread that will close the connection after 2 * MAX RTO passes - timer should reset every time we receive an ACK
        match header_flags(&tpack.header) {
            ACK if tpack.payload.len() == 0 => self.ack(tpack),
            ACK => eprintln!("TIME_WAIT: I shouldn't get a packet with data from my TCP partner"),
            _ => eprintln!(
                "TIME_WAIT: I shouldn't get packet with flags: {}",
                header_flags(&tpack.header)
            ),
        }
        TcpState::TimeWait
    }
    fn close_wait_handler(&mut self, tpack: TcpPacket) -> TcpState {
        //Still accepts acks for its own data, but doesn't expect any incoming data
        //Transitions to LastAck only when application runs close(), so no transitioning happening here
        match header_flags(&tpack.header) {
            ACK if tpack.payload.len() == 0 => self.ack(tpack),
            ACK => eprintln!("CLOSE_WAIT: I shouldn't get a packet with data from my TCP partner"),
            _ => eprintln!(
                "CLOSE_WAIT: I shouldn't get packet with flags: {}",
                header_flags(&tpack.header)
            ),
        }
        TcpState::CloseWait
    }
    fn last_ack_handler(&mut self, tpack: TcpPacket) -> TcpState {
        //Still accepts acks for its own data, but doesn't expect any incoming data
        //Transitions to Closed when ack for FIN previously sent is received
        match header_flags(&tpack.header) {
            ACK if tpack.payload.len() == 0 => {
                if tpack.header.acknowledgment_number == self.seq_num + 1 {
                    self.closed_sender.send(self.sid).unwrap();
                    return TcpState::Closed;
                } else {
                    self.ack(tpack);
                }
            }
            ACK => eprintln!("LAST_ACK: I shouldn't get a packet with data from my TCP partner"),
            _ => eprintln!(
                "LAST_ACK: I shouldn't get packet with flags: {}",
                header_flags(&tpack.header)
            ),
        }
        TcpState::LastAck
    }
    fn closed_handler(&self, _tpack: TcpPacket) -> TcpState {
        //Accepts absolutely nothing, should never execute hopefully
        eprintln!("NOOOOOOOOOOOOOO");
        TcpState::Closed
    }

    //PACKET HANDLING UTILITIES
    ///Absorbs the packet and sends an acknowledgement for it
    fn absorb_and_acknowledge(&mut self, tpack: TcpPacket) {
        //Absorb packet
        self.absorb_packet(tpack);
        //Send acknowledgement for received data
        self.send_flags(ACK);
        println!("Acknowledged received data");
    }
    ///Handles adding the data from the packet to the recv buffer, incrementing ack num, and alert any receiving thread that data was added
    fn absorb_packet(&mut self, tpack: TcpPacket) {
        let mut recv_buf = self.read_buf.get_buf();
        let new_ack = recv_buf.add(tpack.header.sequence_number, tpack.payload);
        self.ack_num = new_ack;
        self.read_buf.alert_ready();
    }
    ///Handles dropping all data associated with sequence numbers less than the ack number of the packet we just received and syncing this with retransmissions
    fn ack(&mut self, tpack: TcpPacket) {
        //Ack data internally within the send buffer
        {
            let mut send_buf = self.write_buf.get_buf();
            send_buf.ack_data(tpack.header.acknowledgment_number);
        }
        // Remove ack'd packets from retransmission queue
        self.ack_rt(tpack.header.acknowledgment_number);
    }

    //SETUP FINISHERS
    ///Sets up socket with knowledge of partner's sequence number
    fn set_init_ack(&mut self, rem_seq_num: u32) {
        //Set ack_num
        self.ack_num = rem_seq_num.clone();
        self.ack_num += 1; //Increment to be next expected value of sequence number
                           //Set Recv Buffer's initial remote sequence number
        let mut read_buf = self.read_buf.get_buf();
        read_buf.set_init_seq(rem_seq_num);
    }
    ///Sets socket's socket ID, should be called when socket is assigned an ID
    pub fn set_sid(slf: Arc<Mutex<Self>>, sid: SocketId) {
        let mut slf = slf.lock().unwrap();
        slf.sid = sid;
    }
    fn ack_rt(&mut self, ack_num: u32) {
        let mut write_buf = self.write_buf.get_buf();
        let retr_queue = &mut write_buf.retr_queue;
        if let Some(measured_rtt) = retr_queue.calculate_rtt(ack_num) {
            // println!("Time until return: {}", measured_rtt.as_millis());
            let mut retr_timer = self.retr_timer.lock().unwrap();
            retr_timer.update_rto(measured_rtt);
            retr_timer.reset();
        }
        retr_queue.remove_acked_segments(ack_num);
    }

    // TODO: CLEAN UP HORRIBLE UGLY ADDING TO RETRANSMISSION QUEUE
    //
    //BUILDING AND SENDING PACKETS
    //
    fn send_flags(&mut self, flags: u8) {
        match self.build_and_send(Vec::new(), flags) {
            Ok(_) => {
                ();
            }
            Err(e) => eprintln!("Error sending flags packet to partner: {}", e),
        }
        if (flags & SYN) != 0 || (flags & FIN) != 0 {
            self.seq_num += 1;
        }
    }
    fn send_data(&mut self, data: Vec<u8>) {
        let data_length = data.len();
        match self.build_and_send(data, ACK) {
            Ok(packet) => {
                self.add_to_queue(
                    packet.header.sequence_number.clone(),
                    packet.payload.clone(),
                    ACK,
                    packet.header.checksum,
                );
                self.seq_num += data_length as u32;
            }
            Err(e) => eprintln!("Error sending data packet to partner: {}", e),
        }
    }
    fn send_probe(&mut self, data: Vec<u8>) {
        self.build_and_send(data, ACK)
            .expect("Error sending probe packe to partner");
        //Don't adjust sequence number for probes
    }
    /// Builds and sends a TCP packet with the given payload and flags
    fn build_and_send(
        &mut self,
        payload: Vec<u8>,
        flags: u8,
    ) -> result::Result<TcpPacket, SendError<PacketBasis>> {
        let new_pack = self.build_packet(payload, flags);
        let pbasis = self.packet_basis(new_pack.clone());
        match self.ip_sender.send(pbasis) {
            Ok(()) => Ok(new_pack),
            Err(e) => Err(e),
        }
    }
    fn add_to_queue(&mut self, seq_num: u32, data: Vec<u8>, flags: u8, checksum: u16) {
        let mut write_buf = self.write_buf.get_buf();
        let retr_queue = &mut write_buf.retr_queue;
        retr_queue.add_segment(seq_num, data.clone(), flags, checksum);
    }
    /// Takes in a TCP header and a u8 representing flags and builds a TCP packet
    fn build_packet(&self, payload: Vec<u8>, flags: u8) -> TcpPacket {
        let window_size = { self.read_buf.get_buf().window() };
        let mut tcp_header = TcpHeader::new(
            self.src_addr.port,
            self.dst_addr.port,
            self.seq_num,
            window_size,
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

    fn send_onwards(
        slf: Arc<Mutex<Self>>,
        buf_update: Arc<Condvar>,
        terminate_send: Arc<AtomicBool>,
    ) -> () {
        //Grab proper resources from slf before relinquishing its lock
        let (write_buf, stop_probing_recver) = {
            let slf = slf.lock().unwrap();
            (
                Arc::clone(&slf.write_buf),
                Arc::clone(&slf.stop_probing_recver),
            )
        };
        let stop_probing_recver = stop_probing_recver.lock().unwrap(); //No other thread should need to use this while this thread is, so good to claim this lock for the duration of the threads existence
                                                                       //Start data sending loop
                                                                       //let mut snd_state = SendState::Sending;
        loop {
            let to_send = {
                //Get data to send
                let mut writer = write_buf.get_buf();
                writer.next_data()
            };
            match to_send {
                NextData::Data(data) => {
                    let mut slf = slf.lock().unwrap();
                    slf.send_data(data);
                }
                NextData::ZeroWindow(probe) => {
                    //Run a thread to zero window probe
                    let slf_clone = Arc::clone(&slf);
                    let done_probing = Arc::new(AtomicBool::new(false));
                    let done_probing_clone = Arc::clone(&done_probing);
                    thread::spawn(move || {
                        Self::zero_window_probe(slf_clone, probe, done_probing_clone)
                    });
                    //Await signal to stop zero window probing
                    stop_probing_recver.recv().unwrap();
                    //Stop zero window probing thread and recover from probing
                    done_probing.store(true, Ordering::SeqCst);
                    {
                        //Account for probe packet now successfully transmitted
                        let mut slf = slf.lock().unwrap();
                        slf.seq_num += 1;
                    }
                }
                NextData::NoData => {
                    //Check to see if we're just done sending
                    if terminate_send.load(Ordering::SeqCst) {
                        return;
                    }
                    //Wait on stuff getting added to buffer
                    let writer = write_buf.get_buf();
                    let _writer = buf_update.wait(writer).unwrap(); //Kinda jank cuz _writer is never used, but is cleanest solution I could think of without adding some stupid boolean flag
                }
            }
        }
    }
    fn zero_window_probe(
        slf: Arc<Mutex<Self>>,
        probe_data: Vec<u8>,
        done_probing: Arc<AtomicBool>,
    ) {
        loop {
            thread::sleep(Duration::from_millis(ZWP_TIMEOUT));
            if done_probing.load(Ordering::SeqCst) {
                return;
            } //Stop probing
            println!("Zero window probing: {probe_data:?}");
            let mut slf = slf.lock().unwrap();
            slf.send_probe(probe_data.clone());
            drop(slf);
        }
    }
    pub fn receive(slf: Arc<Mutex<Self>>, bytes: u16) -> Vec<u8> {
        let read_buf = {
            let slf = slf.lock().unwrap();
            Arc::clone(&slf.read_buf)
        };
        let mut recv_buf: std::sync::MutexGuard<'_, RecvBuf> = read_buf.wait();
        recv_buf.read(bytes)
    }
    fn send_segment(&mut self, seq_num: u32, payload: Vec<u8>, flags: u8, checksum: u16) {
        let my_payload = String::from_utf8(payload.clone()).unwrap();
        println!("Retransmitting payload: {}", my_payload);
        let mut tpack: TcpPacket = self.build_packet(payload, flags);
        println!("Packet ack num: {}", tpack.header.acknowledgment_number);
        tpack.header.checksum = checksum;
        tpack.header.sequence_number = seq_num;
        let pbasis = self.packet_basis(tpack);
        match self.ip_sender.send(pbasis) {
            Ok(()) => (),
            Err(_) => eprintln!("Failed to send retransmission packet"),
        }
    }
    ///Initializes closing procedure
    pub fn close(slf: Arc<Mutex<Self>>) {
        let mut slf = slf.lock().unwrap();
        //Send FIN to TCP rpartner and changes state to FinWait1
        slf.send_flags(FIN);
        let mut state = slf.state.write().unwrap();
        *state = TcpState::FinWait1;
    }
}

/*
enum SendState {
    Sending,
    Probing,
    Waiting,
}
*/
