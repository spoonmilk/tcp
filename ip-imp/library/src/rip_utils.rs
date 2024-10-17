use crate::prelude::*;
use crate::utils::*;

#[derive (Debug, Clone)]
pub struct RipMsg {
    command: u16, // 1 for routing request, 2 for response
    num_entries: u16, // 0 for request, < than 64
    routes: Vec<RipRoute> // As long as num_entries
}

impl RipMsg {
    pub fn new(command: u16, num_entries: u16, routes: Vec<RipRoute>) -> RipMsg {
        RipMsg {
            command,
            num_entries,
            routes
        }
    }
}

#[derive (Debug, Clone)]
pub struct RipRoute {
    cost: u32, // < than 16
    // Examples given with 1.2.3.0/24
    address: u32, // This is a network address > Format 1.2.3.0
    mask: u32 // Netmask > 255.255.0
}

impl RipRoute {
    pub fn new(cost: u32, address: u32, mask: u32) -> RipRoute {
        RipRoute {
            cost,
            address,
            mask
        }
    }
}

// Methods we need 

// Thread in nodes that sends outAdvertisement 
// Update process_packets to deal with RIP packets
    // Add method to handle RIP timeouts
// Method that edits forwarding table of nodes
// Constructing RIP packets

// Things we should add
// New Node field

/// Updates an entry in a node's RIP table according to a RIP route
pub fn route_update(rip_msg: RipRoute , rip_table: &mut HashMap<Ipv4Net, RipRoute>) {
    let new_net = Ipv4Net::with_netmask(Ipv4Addr::from(rip_msg.address), Ipv4Addr::from(rip_msg.mask)).unwrap();

    // Check if route already exists
    if rip_table.contains_key(&new_net) {
        // If it does, check cost
        let org_route = rip_table.get_mut(&new_net).unwrap();
        // Cost is less? Update to new rip message
        if org_route.cost > rip_msg.cost {
            rip_table.insert(new_net.clone(), rip_msg.clone());
        }
        // Cost is more? Check topology
        else if org_route.cost < rip_msg.cost {
            // Topology is different
            if org_route.mask == rip_msg.mask {
                rip_table.insert(new_net.clone(), rip_msg.clone());
            } // Ignore if else
        }
    } else {
        // Add new route
        rip_table.insert(new_net.clone(), rip_msg.clone());
    }
}

/// Updates a node's RIP table according to a RIP message
pub fn update_rip_table(rip_msg: RipMsg, rip_table: &mut HashMap<Ipv4Net, RipRoute>) {
    for route in rip_msg.routes {
        route_update(route, rip_table);
    }
}

/// Takes in the RIP table and returns a RIP update
pub fn form_rip_update(rip_table: &mut HashMap<Ipv4Net, RipRoute>) -> RipMsg {
    let mut routes = Vec::new();
    for (net, route) in rip_table {
        routes.push(RipRoute::new(route.cost, net.network().into(), net.netmask().into()));
    }
    RipMsg::new(2, routes.len() as u16, routes)
}
