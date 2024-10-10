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
) -> Vec<Interface> {
    let mut ret_inters: Vec<Interface> = Vec::new();
    for i in interfaces {
        // Initialize struct information from InterfaceConfig struct
        let my_name: String = i.name;
        let my_prefix: Ipv4Net = i.assigned_prefix;
        let my_ip: Ipv4Addr = i.assigned_ip;
        let my_udp: Ipv4Addr = i.udp_addr;
        let my_port: u16 = i.udp_port;
        let mut my_neighbors: HashMap<Ipv4Addr, u16> = HashMap::new();
        //Initialize bidirectional channel

        /* let chan1 = channel::<CmdType>(CHANNEL_CAPACITY);
        let chan2 = channel::<CmdType>(CHANNEL_CAPACITY);
        let inter_chan = BiChan {
            send: chan1.0,
            recv: chan2.1
        };
        let node_chan = BiChan {
            send: chan2.0,
            recv: chan1.1
        }; */

        // Fill neighbor HashMap from NeighborConfig
        for neigh in &neighbors {
            // Who cares about runtime anyways?
            // Check if neighbor is reachable by this node
            if neigh.interface_name == my_name {
                // If yes, add to my_neighbors
                // TODO: Account for if UDP address is not localhost, for now we just add <dest_addr, udp_port>
                my_neighbors.insert(neigh.dest_addr, neigh.udp_port);
            }
        }
        ret_inters.push(Interface::new(
            my_name.as_str(),
            my_ip,
            my_prefix,
            my_udp,
            my_port,
            my_neighbors,
        ));
    }
    return ret_inters;
}

/// Handles initializing routers, returns to initialize
pub fn initialize(config_info: IPConfig) -> Result<Node> {
    // Create initializer for interfaces
    let interfaces = init_interfaces(config_info.interfaces, config_info.neighbors);
    let (n_type, forwarding_table) = match config_info.routing_mode {
        RoutingType::None => {
            // Node is a host
            let mut route_map = HashMap::new();
            let interface = &interfaces[0];
            // Forwarding to another host
            route_map.insert(interface.v_net, ForwardingOption::Inter(interfaces[0].clone()));
            // Default ; send back to router
            route_map.insert(
                Ipv4Net::new(Ipv4Addr::from(0), 0).unwrap(),
                ForwardingOption::Ip(interface.v_ip),
            );
            let self_addr = Ipv4Net::new(interface.v_ip, 32).unwrap();
            // For self ; consume packet
            route_map.insert(self_addr, ForwardingOption::ToSelf);
            (NodeType::Host, route_map)
        }
        RoutingType::Static => {
            //Node is a router (without RIP)
            let mut route_map: HashMap<Ipv4Net, ForwardingOption> = HashMap::new();
            // Initializing routes in forwarding table
            for route in config_info.static_routes {
                for interface in &interfaces {
                    for neighbor in &interface.neighbors {
                        if route.0.contains(neighbor.0) {
                            route_map.insert(route.0, ForwardingOption::Inter(interface.clone()));
                        }
                    }
                }
            }
            // Creates self route for forwarding
            for interface in &interfaces {
                let self_addr = Ipv4Net::new(interface.v_ip, 32).unwrap();
                route_map.insert(self_addr, ForwardingOption::ToSelf);
            }
            (NodeType::Router, route_map)
        }
        RoutingType::Rip => {
            // Node is an evil router
            panic!("Did you ever hear the tragedy of Darth Plagueis the wise? No. I thought not, It's No story the jedi would tell you. It's a sith legend. Darth Plagueis was a Dark Lord of the sith. He was so powerful, Yet so wise. He could use the force to influence the medi chlorians to create, Life. He had such a knowledge of the Dark side, He could even keep the ones he cared about, From dying. He could actually, Save the ones he cared about from death? The dark side of the force is a pathway to many abilities some consider to be unnatural. Well what happened to him? Darth Plagueis became so powerful that the only thing he feared was losing his power, Which eventually of course he did. Unfortunately, He taught his apprentice everything he knew. Then his apprentice killed him in his sleep. Ironic, He could save others from death, But not himself. Is it possible to learn this power? Not from a jedi.");
        }
    };
    let node = Node::new(n_type, interfaces, forwarding_table);
    Ok(node)
}
