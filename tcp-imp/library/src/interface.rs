use crate::prelude::*;
use crate::utils::*;

#[derive(Debug)]
pub struct Interface {
    pub v_ip: Ipv4Addr,
    pub neighbors: HashMap<Ipv4Addr, u16>,
    pub status: Mutex<InterfaceStatus>, //Only non-static field - represents current status of the interface
    pub udp_sock: UdpSocket
}

impl Interface {
    pub fn new(
        v_ip: Ipv4Addr,
        neighbors: HashMap<Ipv4Addr, u16>,
        udp_port: u16
    ) -> Interface {
        Interface {
            v_ip,
            neighbors,
            status: Mutex::new(InterfaceStatus::Up), //Status always starts as Up
            udp_sock: UdpSocket::bind(format!("127.0.0.1:{}", udp_port)).expect("Unable to bind to port")
        }
    }
    pub fn run(self, chan: BiChan<Packet, InterCmd>) -> () {
        //Make arc of self and clone
        let slf_arc1 = Arc::new(self);
        let slf_arc2 = Arc::clone(&slf_arc1);
        //Unpack BiChan and listen for IPDaemon commands and packets coming across the network
        let sender = chan.send;
        let receiver = chan.recv;
        thread::spawn(move || Interface::node_listen(receiver, slf_arc1));
        Interface::ether_listen( sender, slf_arc2);
    }
    fn node_listen(receiver: Receiver<InterCmd>, slf: Arc<Interface>) -> () {
        loop {
            let received = receiver.recv();
            let status = slf.status.lock().unwrap();
            match received {
                Ok(InterCmd::BuildSend(pb, next_hop, msg_type)) if matches!(*status, InterfaceStatus::Up) => {
                    let builded = slf.build(pb, msg_type);
                    slf.send(builded, next_hop).expect("Error sending packet");
                }
                Ok(InterCmd::Send(pack, next_hop)) if matches!(*status, InterfaceStatus::Up) => slf.send(pack, next_hop).expect("Error sending packet"),
                Ok(InterCmd::ToggleStatus) => Interface::toggle_status(&slf),
                Ok(_) => { println!("I'm down - don't tell me to send crap!"); } //We're currently down, can't send stuff - sorry
                Err(e) => panic!("Error Receiving from almighty IPDaemon: {e:?}"),
            }
        }
    }
    fn ether_listen(sender: Sender<Packet>, slf: Arc<Interface>) -> () {
        loop {
            let pack = match slf.recv() {
                Ok(pack) => pack,
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => return,
                Err(e) => panic!("Error while trying to recv: {e:?}"),
            };
            let status = slf.status.lock().unwrap();
            match *status {
                InterfaceStatus::Up => sender.send(pack).expect("Channel to almighty IPDaemon disconnected"),
                InterfaceStatus::Down => {}
            }
        }
    }
    fn toggle_status(slf_arc: &Arc<Interface>) -> () {
        let mut status = slf_arc.status.lock().unwrap();
        match *status {
            InterfaceStatus::Up => {
                *status = InterfaceStatus::Down;
            }
            InterfaceStatus::Down => {
                *status = InterfaceStatus::Up;
            }
        }
    }
    fn build(&self, pb: PacketBasis, msg_type: bool) -> Packet {
        // Grabbing info from sending interface for header
        let src_ip = self.v_ip;
        let dst_ip = pb.dst_ip;
        let ttl: u8 = 16; // Default TTL from handout
                          // Instantiate payload
        let payload: Vec<u8> = pb.msg;
        let prot_num: IpNumber = if msg_type { 0.into() } else { 200.into() };
        // Create the header
        let mut header = Ipv4Header {
            source: src_ip.octets(),
            destination: dst_ip.octets(),
            time_to_live: ttl,
            total_len: Ipv4Header::MIN_LEN_U16 + (payload.len() as u16),
            protocol: prot_num,
            ..Default::default()
        };
        // Checksum
        header.header_checksum = header.calc_header_checksum();
        return Packet {
            header,
            data: payload,
        }; // Packet built!
    }
    fn send(
        &self,
        pack: Packet,
        next_hop: Ipv4Addr,
    ) -> std::io::Result<()> {
        // Grab neighbor address to send to
        let dst_neighbor = self.neighbors.get(&next_hop).unwrap();
        let mut message = vec![0u8; 20];
        let mut writer = &mut message[..];
        pack.header.write(&mut writer)?;
        message.extend(pack.data);

        // Send
        let sock = &self.udp_sock;
        match sock.send_to(&message, format!("127.0.0.1:{}", dst_neighbor)) {
            // TODO: Do something on Ok? Make error more descriptive?
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
    fn recv(&self) -> Result<Packet> {
        let mut received = false;
        let mut buf: [u8; 1500] = [0; 1500];
        let socket = &self.udp_sock;
        while !received {
            let len = socket.recv(&mut buf)?; // Break if receive
            if len != 0 {
                received = !received;
            }
        }
        match Ipv4Header::from_slice(&buf) {
            Ok((head, rest)) => {
                let len = (head.total_len - 20) as usize;
                let pay: Vec<u8> = Vec::from_iter(rest[0..len].iter().cloned());
                return Ok(Packet {
                    header: head,
                    data: pay,
                });
            }
            Err(_) => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Failed to read received packet error",
                ));
            }
        }
    }
}