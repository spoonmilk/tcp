use crate::prelude::*;
use crate::utils::*;

const INF: u32 = 16;
#[derive(Debug, Clone)]
pub struct RipMsg {
    pub command: u16,          // 1 for routing request, 2 for response
    pub num_entries: u16,      // 0 for request, < than 64
    pub routes: Vec<RipRoute>, // As long as num_entries
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
    mask: u32,    // Netmask > 255.255.0
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

pub fn poison_routes(
    routes: Vec<RipRoute>,
    neighbor_routes: &mut HashMap<Ipv4Addr, Vec<Route>>,
    dst: Ipv4Addr,
) -> Vec<RipRoute> {
    let mut ret_routes: Vec<RipRoute> = Vec::new();
    for route in routes {
        if route.cost == 0 {
            // Locallllll
            ret_routes.push(route);
        } else if route.address == dst.into() {
            ret_routes.push(RipRoute::new(INF, route.address, route.mask));
        } else if neighbor_routes.contains_key(&Ipv4Addr::from(route.address)) {
            ret_routes.push(RipRoute::new(INF, route.address, route.mask));
        } else {
            ret_routes.push(route);
        }
    }
    ret_routes
}

/// Serializes a RIP message to a vector of bytes
pub fn serialize_rip(rip_msg: RipMsg) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&rip_msg.command.to_be_bytes());
    buf.extend_from_slice(&rip_msg.num_entries.to_be_bytes());
    for route in rip_msg.routes {
        buf.extend_from_slice(&route.cost.to_be_bytes());
        buf.extend_from_slice(&route.address.to_be_bytes());
        buf.extend_from_slice(&route.mask.to_be_bytes());
    }
    buf
}

/// Takes in a vector of bytes and returns a RIP message
pub fn deserialize_rip(buf: Vec<u8>) -> RipMsg {
    let mut rip_msg = RipMsg::new(0, 0, Vec::new());
    let mut offset = 0;
    rip_msg.command = u16::from_be_bytes(buf[offset..offset + 2].try_into().unwrap());
    offset += 2;
    rip_msg.num_entries = u16::from_be_bytes(buf[offset..offset + 2].try_into().unwrap());
    offset += 2;
    for _ in 0..rip_msg.num_entries {
        let cost = u32::from_be_bytes(buf[offset..offset + 4].try_into().unwrap());
        offset += 4;
        let address = u32::from_be_bytes(buf[offset..offset + 4].try_into().unwrap());
        offset += 4;
        let mask = u32::from_be_bytes(buf[offset..offset + 4].try_into().unwrap());
        offset += 4;
        rip_msg.routes.push(RipRoute::new(cost, address, mask));
    }
    rip_msg
}
//
// fn check_rip_validity(rip_msg: &RipMsg) -> bool {
//     if rip_msg.command != 1 && rip_msg.command != 2 {
//         false
//     } else if rip_msg.num_entries > (rip_msg.routes.len() as u16) {
//         false
//     } else {
//         true
//     }
// }
//
fn rip_to_route(rip_msg: &mut RipRoute, next_hop: &Ipv4Addr) -> Route {
    Route::new(
        RouteType::Rip,
        Some(rip_msg.cost),
        ForwardingOption::Ip(next_hop.clone()), //Ipv4Addr::from(rip_msg.address))
    )
}

/// Updates an entry in a node's RIP table according to a RIP route
pub fn route_update(
    rip_rt: &mut RipRoute,
    fwd_table: &mut HashMap<Ipv4Net, Route>,
    next_hop: &Ipv4Addr,
) {
    let rip_net =
        Ipv4Net::with_netmask(Ipv4Addr::from(rip_rt.address), Ipv4Addr::from(rip_rt.mask))
            .unwrap()
            .trunc();
    // for debug
    // let thing = rip_to_route(rip_rt, next_hop);
    // println!("{:?}", rip_net);
    // println!("{:?}", thing);
    rip_rt.cost = rip_rt.cost + 1;
    if fwd_table.contains_key(&rip_net) {
        if fwd_table.get(&rip_net).unwrap().next_hop == ForwardingOption::ToSelf || rip_rt.cost == 0
        {
            panic!("Route to self should not be encountered in update")
        }
        let prev_route = fwd_table.get(&rip_net).unwrap();
        match prev_route.cost {
            Some(cost) => {
                if cost > rip_rt.cost {
                    // If lower cost, change next hop
                    fwd_table.insert(rip_net, rip_to_route(rip_rt, next_hop));
                } else if prev_route.next_hop
                    == ForwardingOption::Ip(Ipv4Addr::from(rip_rt.address))
                {
                    // Network topology has changed
                    fwd_table.insert(rip_net, rip_to_route(rip_rt, next_hop));
                } else {
                    //NOTHING IS ADDED
                    ()
                }
            }
            None => panic!("Route cost should not be None"), // Route is to self, do nothing
        }
    } else {
        fwd_table.insert(rip_net, rip_to_route(rip_rt, next_hop));
    }
}
