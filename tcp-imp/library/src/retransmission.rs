use std::collections::VecDeque;
use std::time::{Duration, Instant};

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

// NOTE: These should be 1 millisecond and 60000 milliseconds for turn in
// CONSTANTS
const MIN_RTO: u64 = 150; // Milliseconds
pub const MAX_RTO: u64 = 60000; // Milliseconds
const MAX_RETRANSMISSIONS: u32 = 3;

#[derive(Debug)]
pub struct RetransmissionTimer {
    pub rto: Duration,             // RTO: retransmission timeout
    srtt: Option<Duration>,        // Initially none, see above algo
    rttvar: Option<Duration>,      // Initially none, see above algo
    min_rto: Duration,             // Minimum RTO: 1ms for imp, 150-250ms for testing
    max_rto: Duration,             // Maximum RTO: 100ms(?)
    pub retransmission_count: u32, // Attempt counter ; stop at 3
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
        }
    }
    pub fn update_rto(&mut self, measured_rtt: Duration) {
        const ALPHA: f64 = 1.0 / 8.0;
        const BETA: f64 = 1.0 / 4.0;
        const K: f64 = 4.0;

        let measured_rtt_secs = measured_rtt.as_secs_f64();

        if let (Some(srtt), Some(rttvar)) = (self.srtt, self.rttvar) {
            // Successive RTT calculations
            let srtt_secs = srtt.as_secs_f64();
            let rttvar_secs = rttvar.as_secs_f64();

            let rtt_variation = (measured_rtt_secs - srtt_secs).abs();
            let new_rttvar = (1.0 - BETA) * rttvar_secs + BETA * rtt_variation;
            let new_srtt = (1.0 - ALPHA) * srtt_secs + ALPHA * measured_rtt_secs;

            self.rttvar = Some(Duration::from_secs_f64(new_rttvar));
            self.srtt = Some(Duration::from_secs_f64(new_srtt));
        } else {
            // First RTT measurement
            self.srtt = Some(measured_rtt);
            self.rttvar = Some(measured_rtt / 2);
        }

        // Calculate RTO
        let rto_secs = self.srtt.unwrap().as_secs_f64() + K * self.rttvar.unwrap().as_secs_f64();
        self.rto = Duration::from_secs_f64(rto_secs);
        self.rto = self.rto.clamp(self.min_rto, self.max_rto);
    }
    pub fn do_retransmission(&mut self) {
        self.retransmission_count += 1;
        self.rto *= 2; // RFC 6298 (5.5)
        if self.rto > self.max_rto {
            self.rto = self.max_rto;
        }
    }
    pub fn reset(&mut self) {
        self.retransmission_count = 0;
        // Recalculate RTO based on current srtt and rttvar
        if let (Some(srtt), Some(rttvar)) = (self.srtt, self.rttvar) {
            let rto_secs = srtt.as_secs_f64() + 4.0 * rttvar.as_secs_f64();
            self.rto = Duration::from_secs_f64(rto_secs);
            self.rto = self.rto.clamp(self.min_rto, self.max_rto);
        } else {
            // If no RTT measurements yet, set RTO to initial value
            self.rto = Duration::from_millis(MIN_RTO);
        }
    }
}

#[derive(Debug, Clone)]
pub struct RetrSegment {
    pub seq_num: u32,
    pub payload: Vec<u8>,
    pub flags: u8,
    time_of_send: Instant,
    pub checksum: u16,
    pub retransmission_count: u32,
}

impl RetrSegment {
    /// Creates a new retransmission segment
    pub fn new(seq_num: u32, data: Vec<u8>, flags: u8, checksum: u16) -> RetrSegment {
        RetrSegment {
            seq_num,
            payload: data,
            flags,
            time_of_send: Instant::now(),
            checksum,
            retransmission_count: 0,
        }
    }
    /// Checks if a retransmission segment has timed out
    /// true -> timed out
    /// false -> not timed out
    fn timed_out(&self, current_rto: Duration) -> bool {
        Instant::now().duration_since(self.time_of_send) >= current_rto
    }
    fn update_time_of_send(&mut self) {
        self.time_of_send = Instant::now();
    }
}

#[derive(Debug)]
pub struct RetransmissionQueue {
    pub queue: VecDeque<RetrSegment>,
}

impl RetransmissionQueue {
    pub fn new() -> RetransmissionQueue {
        RetransmissionQueue {
            queue: VecDeque::new(),
        }
    }
    /// Adds a retransmission segment to the queue
    pub fn add_segment(&mut self, seq_num: u32, data: Vec<u8>, flags: u8, checksum: u16) {
        let segment = RetrSegment::new(seq_num, data, flags, checksum);
        self.queue.push_back(segment);
    }
    pub fn mark_sent(&mut self, seq_num: u32) {
        if let Some(segment) = self.queue.iter_mut().find(|s| s.seq_num == seq_num) {
            segment.time_of_send = Instant::now();
        }
    }
    pub fn get_timed_out_segments(&mut self, current_rto: Duration) -> Vec<RetrSegment> {
        let mut timed_out_segments = Vec::new();
        // Use retain_mut to iterate and modify the queue
        self.queue.retain_mut(|segment| {
            if segment.timed_out(current_rto) {
                segment.retransmission_count += 1;

                if segment.retransmission_count >= MAX_RETRANSMISSIONS {
                    // Drop the segment
                    println!(
                        "Dropping segment seq_num={} after {} retransmissions",
                        segment.seq_num, segment.retransmission_count
                    );
                    // Return false to remove the segment from the queue
                    false
                } else {
                    // Update time_of_send for retransmission
                    segment.update_time_of_send();
                    timed_out_segments.push(segment.clone());
                    true // Keep the segment in the queue
                }
            } else {
                true // Keep the segment if not timed out
            }
        });
        timed_out_segments
    }
    pub fn remove_acked_segments(&mut self, ack_num: u32) {
        // println!("Removing segments with seq: {}", ack_num);
        self.queue.retain(|s| s.seq_num >= ack_num);
    }
    pub fn calculate_rtt(&self, ack_num: u32) -> Option<Duration> {
        // Find the oldest unacknowledged segment that is being acknowledged by ack_num
        if let Some(segment) = self.queue.iter().find(|s| s.seq_num < ack_num) {
            Some(Instant::now().duration_since(segment.time_of_send))
        } else {
            None
        }
    }
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}
