fn init_interfaces(
    interfaces: Vec<InterfaceConfig>,
    neighbors: Vec<NeighborConfig>,
) -> HashMap<i32, (Interface, InterfaceRep)> {
    let mut interfaces_reps_map = HashMap::new();
    let mut cnt = 0;
    for i in interfaces {
        // Initialize struct information from InterfaceConfig struct
        let my_name: String = i.name;
        let my_prefix: Ipv4Net = i.assigned_prefix;
        let my_ip: Ipv4Addr = i.assigned_ip;
        let my_udp: Ipv4Addr = i.udp_addr;
        let my_port: u16 = i.udp_port;
        //Initialize bidirectional channels for both the Interface AND it's corresponding InterfaceRep
        let chan1 = channel::<Packet>(CHANNEL_CAPACITY);
        let chan2 = channel::<InterCmd>(CHANNEL_CAPACITY);
        let inter_chan = BiChan {
            send: chan1.0,
            recv: chan2.1
        };
        let inter_rep_chan = BiChan {
            send: chan2.0,
            recv: chan1.1
        };
        // Fill neighbor HashMap AND the InterfaceRep's neighbor vector from NeighborConfig
        let mut my_neighbors: HashMap<Ipv4Addr, u16> = HashMap::new();
        let mut inter_rep_neighbors: Vec<(Ipv4Addr, u16)> = Vec::new();
        for neigh in &neighbors {
            // Who cares about runtime anyways?
            // Check if neighbor is reachable by this node
            if neigh.interface_name == my_name {
                // If yes, add to my_neighbors
                // TODO: Account for if UDP address is not localhost, for now we just add <dest_addr, udp_port>
                my_neighbors.insert(neigh.dest_addr, neigh.udp_port);
                inter_rep_neighbors.push((neigh.dest_addr, neigh.udp_port));
            }
        }
        //Add the completed Interfaces and InterfaceReps to their corresponding vectors for return
        interfaces_reps_map.insert(
            cnt,
            (Interface::new(
                my_name.clone(),
                my_ip,
                my_prefix,
                my_udp,
                my_port,
                my_neighbors,
                inter_chan,
            ),
            InterfaceRep::new(
                my_name, 
                my_prefix,
                inter_rep_neighbors,
                inter_rep_chan,
            ))
        );
        cnt += 1;
    }
    interfaces_reps_map
}

let mut route_map = HashMap::new();
// Initializing routes in forwarding table
for route in config_info.static_routes {
    let keys: Vec<_> = interface_reps_map.keys().cloned().collect(); //Keys could be different on each iteration, so needs to be formulated here
    for key in &keys {
        let mut flag = false;
        let (interface, _) = interface_reps_map.get(key).unwrap();
        for neighbor in &interface.neighbors {
            if route.0.contains(neighbor.0) {
                let (interface, interface_rep) = interface_reps_map.remove(key).unwrap();
                route_map.insert(
                    route.0,
                    Route::new(RouteType::Local, Some(0), ForwardingOption::Inter((&interface_rep).name.clone())) //Route is local because it is a route to a connected interface, so cost is 0
                );
                interface_reps.insert((&interface_rep).name.clone(), interface_rep);
                interfaces.push(interface);
                flag = true;
                break
            }
        }
        if flag { break }
    }
}
//interface_reps_map is DEPLETED by this point - interfaces is now GROWN
// Creates self route for forwarding
for interface in &interfaces {
    let self_addr = Ipv4Net::new(interface.v_ip, 32).unwrap();
    route_map.insert(self_addr, Route::new(RouteType::ToSelf, None, ForwardingOption::ToSelf)); //Route is toSelf, so cost is None
}


use crate::prelude::*;
use crate::ip_data_types::*;
use crate::utils::*;

fn init_interfaces(interfaces: Vec<InterfaceConfig>, neighbors: Vec<NeighborConfig>) -> (Vec<Interface>, HashMap<String, InterfaceRep>) {
    let mut created_interfaces = Vec::new();
    let mut interface_reps = HashMap::new();
    for inter_conf in interfaces {
        //Initialize bidirectional channels for both the Interface AND it's corresponding InterfaceRep
        let (inter_chan, inter_rep_chan) = make_bichans();
        // Fill neighbor HashMap AND the InterfaceRep's neighbor vector from NeighborConfig
        let mut inter_neighbors: HashMap<Ipv4Addr, u16> = HashMap::new();
        let mut inter_rep_neighbors: Vec<(Ipv4Addr, u16)> = Vec::new();
        for neigh in &neighbors {
            // Check if neighbor is reachable by this node
            if neigh.interface_name == inter_conf.name {
                // If yes, add to my_neighbors
                inter_neighbors.insert(neigh.dest_addr, neigh.udp_port);
                inter_rep_neighbors.push((neigh.dest_addr, neigh.udp_port));
            }
        }
        //Add the completed Interfaces and InterfaceReps to their corresponding vectors for return
        created_interfaces.push(Interface::new(
                inter_conf.name.clone(),
                inter_conf.assigned_ip,
                inter_conf.assigned_prefix.clone(),
                inter_conf.udp_addr,
                inter_conf.udp_port,
                inter_neighbors,
                inter_chan
            ));
        interface_reps.insert(inter_conf.name.clone(), InterfaceRep::new(
            inter_conf.name, 
            inter_conf.assigned_prefix, 
            inter_rep_neighbors, 
            inter_rep_chan
        ));
    }
    (created_interfaces, interface_reps)
}

///Creates a pair of connected BiChans for corresponding interfaces and interfaceReps
fn make_bichans() -> (BiChan<Packet, InterCmd>, BiChan<InterCmd, Packet>) {
    let chan1 = channel::<Packet>(CHANNEL_CAPACITY);
    let chan2 = channel::<InterCmd>(CHANNEL_CAPACITY);
    let inter_chan = BiChan {
        send: chan1.0,
        recv: chan2.1
    };
    let inter_rep_chan = BiChan {
        send: chan2.0,
        recv: chan1.1
    };
    (inter_chan, inter_rep_chan)
}

// Handles initializing routers, returns to initialize
pub fn initialize(config_info: IPConfig) -> Result<Node> {
    // Create list of interfaces and corresponding hashmap of interfaceReps (keys are names of interfaceReps)
    let (interfaces, interface_reps) = init_interfaces(config_info.interfaces, config_info.neighbors);
    //Find node type
    let n_type = match config_info.routing_mode {
        RoutingType::None => NodeType::Host,
        RoutingType::Static => NodeType::Router,
        RoutingType::Rip => NodeType::Router
    };
    //Create forwarding table
    let mut forwarding_table = HashMap::new();
    add_static_routes(&mut forwarding_table, config_info.static_routes);
    add_local_routes(&mut forwarding_table, &interfaces);
    add_toself_routes(&mut forwarding_table, &interfaces);
    //Create and return node
    println!("{forwarding_table:?}");
    let node = Node::new(n_type, interfaces, interface_reps, forwarding_table);
    Ok(node)
}

fn add_static_routes(fwd_table: &mut HashMap<Ipv4Net, Route>, static_routes: Vec<StaticRoute>) -> () {
    for (net_prefix, inter_addr) in static_routes {
        let new_route = Route::new(RouteType::Static, None, ForwardingOption::Ip(inter_addr.clone()));
        fwd_table.insert(net_prefix.clone(), new_route);
    }
}

fn add_local_routes(fwd_table: &mut HashMap<Ipv4Net, Route>, interfaces: &Vec<Interface>) -> () {
    for interface in interfaces {
        let new_route = Route::new(RouteType::Local, Some(0), ForwardingOption::Inter(interface.name.clone()));
        fwd_table.insert(interface.v_net.clone(), new_route);
    }
}

fn add_toself_routes(fwd_table: &mut HashMap<Ipv4Net, Route>, interfaces: &Vec<Interface>) -> () {
    for interface in interfaces {
        let new_route = Route::new(RouteType::ToSelf, None, ForwardingOption::ToSelf);
        let self_addr = Ipv4Net::new((&interface).v_ip, 32).unwrap();
        fwd_table.insert(self_addr, new_route);
    }
}