

pub struct RipMsg {
    command: u16, // 1 for routing request, 2 for response
    num_entries: u16, // 0 for request, < than 64
    routes: Vec<RipRoute> // As long as num_entries
}

pub struct RipRoute {
    cost: u32, // < than 16
    // Examples given with 1.2.3.0/24
    address: u32, // This is a network address > Format 1.2.3.0
    mask: u32 // Netmask > 255.255.0
}

// Methods we need 

// Thread in nodes that sends outAdvertisement 
// Update process_packets to deal with RIP packets
    // Add method to handle RIP timeouts
// Method that edits forwarding table of nodes
// Constructing RIP packets

// Things we should add
// New Node field