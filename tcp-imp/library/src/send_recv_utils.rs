use crate::prelude::*;
use crate::retransmission::*;

const MAX_MSG_SIZE: usize = 1418; //+ 82 for headers = 1500 total max packet size
const BUFFER_CAPACITY: usize = 65535;

#[derive(Debug)]
pub struct SyncBuf<T: TcpBuffer> {
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

pub trait TcpBuffer {
    fn ready(&self) -> bool;
}

#[derive(Debug)]
pub struct SendBuf {
    pub circ_buffer: CircularBuffer<BUFFER_CAPACITY, u8>,
    //una: usize, Don't need, b/c una will always be 0 technically
    pub nxt: usize, // Pointer to next byte to be sent ; NOTE, UPDATE AS BYTES DRAINED
    //lbw: usize Don't need b/c lbw will always be circ_buffer.len() technically
    pub rem_window: u16,
    num_acked: u32,
    our_init_seq: u32,
    pub probing: bool, //Identifies whether or not we are currently probing - a little kludgy
    stop_probing_sender: Sender<()>,
    pub retr_queue: RetransmissionQueue,
}

impl TcpBuffer for SendBuf {
    //Ready when buffer is not full
    fn ready(&self) -> bool {
        self.circ_buffer.len() != self.circ_buffer.capacity()
    }
}

impl SendBuf {
    pub fn new(our_init_seq: u32, stop_probing_sender: Sender<()>) -> SendBuf {
        SendBuf {
            circ_buffer: CircularBuffer::new(),
            nxt: 0,
            rem_window: 0,
            num_acked: 0,
            our_init_seq, //OURS
            probing: false,
            stop_probing_sender,
            retr_queue: RetransmissionQueue::new(),
        }
    }
    ///Fills up the circular buffer with the data in filler until the buffer is full,
    ///then returns the original input filler vector drained of the values added to the circular buffer
    pub fn fill_with(&mut self, mut filler: Vec<u8>) -> Vec<u8> {
        let available_spc = self.circ_buffer.capacity() - self.circ_buffer.len();
        let to_add = filler
            .drain(..std::cmp::min(available_spc, filler.len()))
            .collect::<Vec<u8>>();

        self.circ_buffer.extend_from_slice(&to_add[..]); 
        filler
    }
    ///Returns a vector of data to be put in the next TcpPacket to send, taking into account the input window size of the receiver
    ///This vector contains as many bytes as possible up to the maximum payload size (1500)
    pub fn next_data(&mut self) -> NextData {
        if self.rem_window == 0 {
            // Zero window probing
            self.probing = true;
            let data = if self.circ_buffer.len() > self.nxt {
                // Normal case: resend one byte from the buffer to probe
                self.see_amount(1)
            } else if self.circ_buffer.len() > 0 {
                // If nxt points beyond len, but there is data, adjust nxt if needed:
                let available = self.circ_buffer.len() - self.nxt;
                self.see_amount(std::cmp::min(1, available))
            } else {
                // No data at all, insert a dummy byte for probing
                return NextData::NoData;
            };
            return NextData::ZeroWindow(data);
        } else {
            // Normal data
            let greatest_constraint = std::cmp::min(self.rem_window as usize, MAX_MSG_SIZE);
            let data = self.take_amount(greatest_constraint);
            self.rem_window -= data.len() as u16;
            if data.is_empty() {
                NextData::NoData
            } else {
                NextData::Data(data)
            }
        }
    }
    
    ///Only used privately; same as see_amount but increments the nxt pointer
    fn take_amount(&mut self, amount: usize) -> Vec<u8> {
        let data = self.see_amount(amount);
        self.nxt += data.len();
        data
    }
    ///Only used privately; clones up to (as much as possible) the specified amount out of the circular buffer (after the nxt pointer)
    pub fn see_amount(&self, amount: usize) -> Vec<u8> { //ONLY PUB FOR DEBUGGING
        let greatest_constraint = cmp::min(amount, self.circ_buffer.len() - self.nxt);
        let upper_bound = self.nxt + greatest_constraint;
        self.circ_buffer
            .range(self.nxt..upper_bound)
            .cloned()
            .collect()
    }
    ///Acknowledges (drops) all sent bytes up to the one indicated by most_recent_ack
    /// Acknowledges (drops) all sent bytes up to the one indicated by most_recent_ack
    pub fn ack_data(&mut self, most_recent_ack: u32) {
        let expected_ack = self.num_acked + self.our_init_seq + 1;
        if most_recent_ack < expected_ack {
            // Old ack: it acknowledges no new data
            return;
        }
    
        let mut relative_ack = most_recent_ack - expected_ack;
        let acked_bytes = relative_ack as usize;
    
        // If we were probing and the ACK covers beyond nxt, it means our probe byte got acknowledged.
        if self.probing && acked_bytes > self.nxt {
            self.probing = false;
            // The probe byte is considered sent from nxt, so increment nxt by one 
            // since that byte is now acknowledged.
            let _ = self.stop_probing_sender.send(()); // Ignore if fails
            // Adjust relative_ack since that one byte was a probe
        }
    
        // Handle FIN acknowledgment if needed
        // (If you have a FIN at nxt+1, and it's acked, decrement relative_ack by 1.)
        if acked_bytes == self.nxt + 1 {
            relative_ack -= 1;
        }
    
        let acked_data = relative_ack as usize;
        let available_data = self.circ_buffer.len();
        let actual_acked = std::cmp::min(acked_data, available_data);
    
        // Never let nxt go negative
        if actual_acked >= self.nxt {
            // All currently "in-flight" data plus possibly the probe is acked
            self.nxt = 0;
        } else {
            self.nxt -= actual_acked;
        }
    
        // Drain out the acknowledged data
        // This removes the acknowledged bytes from the front of circ_buffer
        self.circ_buffer.drain(..actual_acked);
        self.num_acked += actual_acked as u32;
    }

    ///Updates the SendBuf's internal tracker of how many more bytes can be sent before filling the reciever's window
    pub fn update_window(&mut self, new_window: u16) {
        self.rem_window = new_window;
    }
    // pub fn check_timeouts(&mut self, current_rto: Duration) -> Vec<RetrSegment> {
    //     let timed_out_segments: Vec<RetrSegment> =
    //         { self.retr_queue.get_timed_out_segments(current_rto) };
    //     timed_out_segments
    // }
    pub fn still_sending(&self) -> bool {
        self.circ_buffer.len() == 0 && self.nxt == 0 && !self.probing && self.retr_queue.is_empty()
    }
}

pub enum NextData {
    Data(Vec<u8>),
    ZeroWindow(Vec<u8>),
    NoData,
}

#[derive(Debug)]
pub struct RecvBuf {
    circ_buffer: CircularBuffer<BUFFER_CAPACITY, u8>,
    //lbr: usize Don't need, lbr will always be 0
    //nxt: usize Don't need, nxt will always be circ_buffer.len()
    early_arrivals: PayloadMap,
    bytes_read: u32,
    rem_init_seq: u32,
    final_seq: Option<u32>
}

impl TcpBuffer for RecvBuf {
    //Ready when buffer has some elements
    fn ready(&self) -> bool {
        let received_fin = if let Some(_) = self.final_seq { true } else { false };
        self.circ_buffer.len() != 0 || received_fin
    }
}

impl RecvBuf {
    pub fn new() -> RecvBuf {
        RecvBuf {
            circ_buffer: CircularBuffer::new(),
            early_arrivals: PayloadMap::new(),
            bytes_read: 0,
            rem_init_seq: 0, //We don't know yet *shrug* - gets set once and then is never edited
            final_seq: None
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
        //println!("sequence number: {}\nexpected sequence number: {}", seq_num, self.expected_seq());
        match seq_num.cmp(&self.expected_seq()) {
            cmp::Ordering::Equal => {
                let data_slice = match data.len() > self.window() as usize {
                    true => &data[..self.window() as usize],
                    false => &data[..],
                };
                self.circ_buffer.extend_from_slice(data_slice);
                if let Some(next_data) = self.early_arrivals.remove(&self.expected_seq()) {
                    return self.add(self.expected_seq(), next_data);
                }
            }
            cmp::Ordering::Less => {} //Drop packet, contains stale data
            cmp::Ordering::Greater => {
                if data.len() <= self.window() as usize { self.early_arrivals.insert(seq_num, data) }
                else {} //Drop packet, we don't have space
            } //Early arrival, add it to early arrival hashmap
        }
        self.expected_seq()
    }
    ///Returns the next expected sequence number - only used privately, self.add() returns next sequence number too for public use
    fn expected_seq(&self) -> u32 {
        self.rem_init_seq + self.bytes_read + ((self.circ_buffer.len() + 1) as u32)
    }
    ///Returns the buffer's current window size
    pub fn window(&self) -> u16 {
        (self.circ_buffer.capacity() - self.circ_buffer.len() - self.early_arrivals.len()) as u16
    }
    ///Returns a boolean representing whether or not there is data the buffer still expects to receive
    pub fn can_receive(&self) -> bool {
        match self.final_seq {
            Some(fin_seq_num) => self.expected_seq() < fin_seq_num || self.circ_buffer.len() != 0, //Can receive if there are still packets out there OR if we still have stuff in our buffer
            None => true
        }
    }

    pub fn set_init_seq(&mut self, seq_num: u32) {
        self.rem_init_seq = seq_num;
    }
    pub fn set_final_seq(&mut self, fin_seq_num: u32) {
        self.final_seq = Some(fin_seq_num);
    }
}

//A wrapper for a HashMap<u32, Vec<u8>> that keeps track of the cumulative size of the data stored in the map
//accessable via the len method like a vector
#[derive(Debug)]
struct PayloadMap {
    hash_map: HashMap<u32, Vec<u8>>, //Could be made pub if we ever need to call methods on the hashmap that aren't insert() or remove()
    size: usize,
}

impl PayloadMap {
    pub fn new() -> PayloadMap {
        PayloadMap {
            hash_map: HashMap::new(),
            size: 0,
        }
    }
    pub fn insert(&mut self, key: u32, val: Vec<u8>) {
        self.size += val.len();
        self.hash_map.insert(key, val);
    }
    pub fn remove(&mut self, key: &u32) -> Option<Vec<u8>> {
        let payload = self.hash_map.remove(key);
        if let Some(data) = &payload {
            self.size -= data.len()
        }
        payload
    }
    pub fn len(&self) -> usize {
        self.size
    }
}
