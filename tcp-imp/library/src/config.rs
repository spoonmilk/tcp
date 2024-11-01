use crate::interface::*;
// use crate::ip_daemon::*;
use crate::prelude::*;
use crate::utils::*;
use crate::backends::{ HostBackend, RouterBackend, Backend };

fn init_interfaces(
    interfaces: Vec<InterfaceConfig>,
    neighbors: Vec<NeighborConfig>
) -> InterfaceTable {
    let mut interface_reps = HashMap::new();
    for inter_conf in interfaces {
        let (inter_chan, inter_rep_chan) = make_bichans();
        // Fill neighbor HashMap AND the InterfaceRep's neighbor vector from NeighborConfig
        let mut inter_neighbors: HashMap<Ipv4Addr, u16> = HashMap::new();
        let mut inter_rep_neighbors: Vec<(Ipv4Addr, u16)> = Vec::new();
        for neigh in &neighbors {
            // Check if neighbor is reachable by this IPDaemon
            if neigh.interface_name == inter_conf.name {
                // If yes, add to my_neighbors
                inter_neighbors.insert(neigh.dest_addr, neigh.udp_port);
                inter_rep_neighbors.push((neigh.dest_addr, neigh.udp_port));
            }
        }
        //Add the completed Interfaces and InterfaceReps to their corresponding vectors for return
        let new_interface = Interface::new(
            inter_conf.assigned_ip.clone(),
            inter_neighbors,
            inter_conf.udp_port
        );
        thread::spawn(move || new_interface.run(inter_chan));
        interface_reps.insert(
            inter_conf.name.clone(),
            InterfaceRep::new(
                inter_conf.name,
                inter_conf.assigned_prefix, //.trunc(),
                inter_conf.assigned_ip,
                inter_rep_neighbors,
                inter_rep_chan.send
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
pub fn initialize(config_info: IPConfig) -> Result<(Backend, Receiver<String>)> {
    // Create hashmap of interfaceReps (keys are names of interfaceReps)
    let interface_reps = init_interfaces(config_info.interfaces, config_info.neighbors);
    //Create forwarding table
    let mut forwarding_table = HashMap::new();
    add_static_routes(&mut forwarding_table, config_info.static_routes);
    add_local_routes(&mut forwarding_table, &interface_reps);
    add_toself_routes(&mut forwarding_table, &interface_reps);

    // Create RIP neighbors table
    let mut rip_table: HashMap<Ipv4Addr, Vec<Route>> = HashMap::new();
    match config_info.rip_neighbors {
        Some(rip_neighbors) => add_rip_neighbors(&mut rip_table, rip_neighbors),
        None => (),
    }
    match config_info.routing_mode {
        RoutingType::Rip => {
            // let handler_table = HashMap::new();
            let ip_sender = channel::<PacketBasis>().0; // Create a new sender channel
            Ok((
                Backend::Router(
                    RouterBackend::new(
                        Arc::new(RwLock::new(interface_reps)), // Wrap interface_reps in Arc and RwLock
                        Arc::new(RwLock::new(forwarding_table)), // Wrap forwarding_table in Arc and RwLock
                        Arc::new(RwLock::new(rip_table)), // Wrap rip_table in Arc and RwLock
                        ip_sender // Pass ip_sender as the third argument
                    )
                ),
                channel::<String>().1,
            ))
        }
        RoutingType::Static => {
            // let handler_table = HashMap::new();
            let ip_sender = channel::<PacketBasis>().0; // Create a new sender channel
            Ok((
                Backend::Host(
                    HostBackend::new(
                        Arc::new(RwLock::new(interface_reps)), // Wrap interface_reps in Arc and RwLock
                        Arc::new(RwLock::new(forwarding_table)), // Wrap forwarding_table in Arc and RwLock
                        ip_sender // Pass ip_sender as the third argument
                    )
                ),
                channel::<String>().1,
            ))
        }
        RoutingType::None => panic!("Should never encounter config with router type none."),
    }
}

fn add_static_routes(fwd_table: &mut ForwardingTable, static_routes: Vec<StaticRoute>) -> () {
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

fn add_local_routes(fwd_table: &mut ForwardingTable, interface_reps: &InterfaceTable) -> () {
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

fn add_toself_routes(fwd_table: &mut ForwardingTable, interface_reps: &InterfaceTable) -> () {
    let interface_reps = interface_reps.values();
    for interface_rep in interface_reps {
        let new_route = Route::new(RouteType::ToSelf, None, ForwardingOption::ToSelf);
        let self_addr = Ipv4Net::new((&interface_rep).v_ip, 32).unwrap();
        fwd_table.insert(self_addr, new_route);
    }
}
