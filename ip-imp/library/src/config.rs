use crate::prelude::*;
use crate::ip_data_types::*;
use crate::utils::*;

/*
pub struct Node {
        n_type: NodeType,
        interfaces: Vec<Interface>,
        forwarding_table: HashMap<Ipv4Net, ForwardingOption>
    }
 */

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
        let mut inter_rep_neighbors: Vec<Ipv4Addr> = Vec::new();
        for neigh in &neighbors {
            // Who cares about runtime anyways?
            // Check if neighbor is reachable by this node
            if neigh.interface_name == my_name {
                // If yes, add to my_neighbors
                // TODO: Account for if UDP address is not localhost, for now we just add <dest_addr, udp_port>
                my_neighbors.insert(neigh.dest_addr, neigh.udp_port);
                inter_rep_neighbors.push(neigh.dest_addr);
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
                inter_rep_neighbors,
                inter_rep_chan,
            ))
        );
        cnt += 1;
    }
    interfaces_reps_map
}

// Handles initializing routers, returns to initialize
pub fn initialize(config_info: IPConfig) -> Result<Node> {
    // Create initializer for interfaces
    let mut interface_reps_map = init_interfaces(config_info.interfaces, config_info.neighbors); //Contains pairings of all Interfaces and their corresponding InterfaceReps - will be depleted as processed
    let mut interfaces = Vec::new(); //Will grow to hold the complete list of interfaces that the node has after procesing interface_reps_map
    let mut interface_reps = HashMap::new();
    //Create forwarding table
    let (n_type, forwarding_table) = match config_info.routing_mode {
        RoutingType::None => {
            // Node is a host
            let mut route_map = HashMap::new();
            let (interface, interface_rep) = interface_reps_map.remove(&0).unwrap();
            // Forwarding to another host
            route_map.insert(
                (&interface).v_net, 
                Route::new(RouteType::Local, Some(0), ForwardingOption::Inter((&interface_rep).name.clone())) //Route is local because it is a route to a connected interface, so cost is 0
            );
            interface_reps.insert((&interface_rep).name.clone(), interface_rep);
            // Default ; send back to router
            route_map.insert(
                Ipv4Net::new(Ipv4Addr::from(0), 0).unwrap(),
                Route::new(RouteType::Static, None, ForwardingOption::Ip((&interface).v_ip)), //Default route, so static and has no cost
            );
            // For self ; consume packet
            let self_addr = Ipv4Net::new((&interface).v_ip, 32).unwrap();
            route_map.insert(self_addr, Route::new(RouteType::ToSelf, None, ForwardingOption::ToSelf)); //Route is toSelf, so cost is None
            // Add the sole interface to the interfaces vector and return
            interfaces.push(interface);
            (NodeType::Host, route_map)
        }
        RoutingType::Static => {
            //Node is a router (without RIP)
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
            (NodeType::Router, route_map)
        }
        RoutingType::Rip => {
            // Node is an evil router
            panic!("Did you ever hear the tragedy of Darth Plagueis the wise? No. I thought not, It's No story the jedi would tell you. It's a sith legend. Darth Plagueis was a Dark Lord of the sith. He was so powerful, Yet so wise. He could use the force to influence the medi chlorians to create, Life. He had such a knowledge of the Dark side, He could even keep the ones he cared about, From dying. He could actually, Save the ones he cared about from death? The dark side of the force is a pathway to many abilities some consider to be unnatural. Well what happened to him? Darth Plagueis became so powerful that the only thing he feared was losing his power, Which eventually of course he did. Unfortunately, He taught his apprentice everything he knew. Then his apprentice killed him in his sleep. Ironic, He could save others from death, But not himself. Is it possible to learn this power? Not from a jedi.");
        }
    };
    let node = Node::new(n_type, interfaces, interface_reps, forwarding_table);
    Ok(node)
}
