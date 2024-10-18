use crate::prelude::*;
use crate::utils::*;

#[derive(Debug, Clone)]
pub struct RipMsg {
    command: u16, // 1 for routing request, 2 for response
    num_entries: u16, // 0 for request, < than 64
    routes: Vec<RipRoute>, // As long as num_entries
}

impl RipMsg {
    pub fn new(command: u16, num_entries: u16, routes: Vec<RipRoute>) -> RipMsg {
        RipMsg {
            command,
            num_entries,
            routes,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RipRoute {
    cost: u32, // < than 16
    // Examples given with 1.2.3.0/24
    address: u32, // This is a network address > Format 1.2.3.0
    mask: u32, // Netmask > 255.255.0
}

impl RipRoute {
    pub fn new(cost: u32, address: u32, mask: u32) -> RipRoute {
        RipRoute {
            cost,
            address,
            mask,
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

pub fn table_to_rip(forwarding_table: &mut HashMap<Ipv4Net, Route>) -> RipMsg {
    let mut routes = Vec::new();
    for (net, route) in forwarding_table {
        match route.cost {
            Some(cost) => routes.push(RipRoute::new(cost, net.network().into(), net.netmask().into())),
            None => routes.push(RipRoute::new(0, net.network().into(), net.netmask().into())),
        }
    }
    RipMsg::new(2, routes.len() as u16, routes)
}

pub fn serialize_rip(rip_msg: RipMsg) -> Vec<u8> {
    let mut ret = Vec::new();
    ret.extend_from_slice(&rip_msg.command.to_be_bytes());
    ret.extend_from_slice(&rip_msg.num_entries.to_be_bytes());
    for route in rip_msg.routes {
        ret.extend_from_slice(&route.cost.to_be_bytes());
        ret.extend_from_slice(&route.address.to_be_bytes());
        ret.extend_from_slice(&route.mask.to_be_bytes());
    }
    ret
}

fn rip_to_route (rip_msg: RipRoute) -> Route {
    Route::new(RouteType::Rip, Some(rip_msg.cost), ForwardingOption::Ip(Ipv4Addr::from(rip_msg.address)))
}

/// Updates an entry in a node's RIP table according to a RIP route
pub fn route_update(rip_rt: RipRoute, fwd_table: &mut HashMap<Ipv4Net, Route>) {
    let rip_net = Ipv4Net::with_netmask(
        Ipv4Addr::from(rip_rt.address),
        Ipv4Addr::from(rip_rt.mask)
    ).unwrap();

    if fwd_table.contains_key(&rip_net) {
        let prev_route = fwd_table.get(&rip_net).unwrap();

        match prev_route.cost {
            Some(cost) => {
                if cost > rip_rt.cost {
                    // If lower cost, change next hop
                    fwd_table.insert(rip_net, rip_to_route(rip_rt));
                } else {
                    if prev_route.next_hop == ForwardingOption::Ip(Ipv4Addr::from(rip_rt.address)) {
                        // Network topology has changed
                        fwd_table.insert(rip_net, rip_to_route(rip_rt));
                    }
                }
            }, 
            None => (), // Route is to self, do nothing
        }
    } else {
        fwd_table.insert(rip_net, rip_to_route(rip_rt));
    }
}

/// Updates a node's RIP table according to a RIP message
pub fn update_fwd_table(rip_msg: RipMsg, fwd_table: &mut HashMap<Ipv4Net, Route>) {
    for route in rip_msg.routes {
        route_update(route, fwd_table);
    }
}