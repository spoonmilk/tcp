use crate::prelude::*;
use crate::ip_data_types::*;
use crate::utils::*;
use crate::interface::*;

fn init_interfaces(
    interfaces: Vec<InterfaceConfig>,
    neighbors: Vec<NeighborConfig>
) -> HashMap<String, InterfaceRep> {
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
        let new_interface =  Interface::new(
            inter_conf.name.clone(),
            inter_conf.assigned_ip.clone(),
            inter_conf.assigned_prefix.clone(),//.trunc(),
            inter_conf.udp_addr,
            inter_conf.udp_port,
            inter_neighbors,
            //inter_chan
        );
        thread::spawn(move || new_interface.run(inter_chan));
        interface_reps.insert(
            inter_conf.name.clone(),
            InterfaceRep::new(
                inter_conf.name,
                inter_conf.assigned_prefix,//.trunc(),
                inter_conf.assigned_ip,
                inter_rep_neighbors,
                inter_rep_chan
            )
        );
    }
    interface_reps
}

///Creates a pair of connected BiChans for corresponding interfaces and interfaceReps
fn make_bichans() -> (BiChan<Packet, InterCmd>, BiChan<InterCmd, Packet>) {
    let chan1 = channel::<Packet>();
    let chan2 = channel::<InterCmd>();
    let inter_chan = BiChan {
        send: chan1.0,
        recv: chan2.1,
    };
    let inter_rep_chan = BiChan {
        send: chan2.0,
        recv: chan1.1,
    };
    (inter_chan, inter_rep_chan)
}

// Handles initializing routers, returns to initialize
pub fn initialize(config_info: IPConfig) -> Result<Node> {
    // Create hashmap of interfaceReps (keys are names of interfaceReps)
    let interface_reps = init_interfaces(
        config_info.interfaces,
        config_info.neighbors
    );
    //Find node type
    let n_type = match config_info.routing_mode {
        RoutingType::None => panic!("Encountered None routing mode, unused."),
        RoutingType::Static => NodeType::Host,
        RoutingType::Rip => NodeType::Router,
    };
    //Create forwarding table
    let mut forwarding_table = HashMap::new();
    add_static_routes(&mut forwarding_table, config_info.static_routes);
    add_local_routes(&mut forwarding_table, &interface_reps);
    add_toself_routes(&mut forwarding_table, &interface_reps);

    // Create RIP neighbors table
    let mut rip_table: HashMap<Ipv4Addr, Vec<Route>> = HashMap::new();
    match config_info.rip_neighbors {
        Some(rip_neighbors) => add_rip_neighbors(&mut rip_table, rip_neighbors),
        None => ()
    }

    //Create and return node
    //let node = Node::new(n_type, interfaces, interface_reps, forwarding_table, rip_table); //PLACEHOLDER for now; FIX later
    let node = Node::new(n_type, interface_reps, forwarding_table, rip_table); //PLACEHOLDER for now; FIX later
    Ok(node)
}

fn add_static_routes(
    fwd_table: &mut HashMap<Ipv4Net, Route>,
    static_routes: Vec<StaticRoute>
) -> () {
    for (net_prefix, inter_addr) in static_routes {
        let new_route = Route::new(
            RouteType::Static,
            None,
            ForwardingOption::Ip(inter_addr.clone())
        );
        fwd_table.insert(net_prefix.clone(), new_route);
    }
}

fn add_rip_neighbors(
    rip_table: &mut HashMap<Ipv4Addr, Vec<Route>>,
    rip_neighbors: Vec<Ipv4Addr>
) -> () {
    for neighbor in rip_neighbors {
        rip_table.insert(neighbor, Vec::new());
    }   
}

fn add_local_routes(fwd_table: &mut HashMap<Ipv4Net, Route>, interface_reps: &HashMap<String, InterfaceRep>) -> () {
    let interface_reps = interface_reps.values();
    for interface_rep in interface_reps {
        let new_route = Route::new(
            RouteType::Local,
            Some(0),
            ForwardingOption::Inter(interface_rep.name.clone())
        );
        fwd_table.insert(interface_rep.v_net.clone(), new_route);
    }
}

fn add_toself_routes(fwd_table: &mut HashMap<Ipv4Net, Route>, interface_reps: &HashMap<String, InterfaceRep>) -> () {
    let interface_reps = interface_reps.values();
    for interface_rep in interface_reps {
        let new_route = Route::new(RouteType::ToSelf, None, ForwardingOption::ToSelf);
        let self_addr = Ipv4Net::new((&interface_rep).v_ip, 32).unwrap();
        fwd_table.insert(self_addr, new_route);
    }
}
