use crate::prelude::*;
use crate::rip_utils::*;
use crate::utils::*;
use crate::vnode_traits::VnodeIpDaemon;

pub trait RipDaemon: VnodeIpDaemon {
    fn rip_neighbors(&self) -> &RipNeighbors;
    /// Periodically broadcasts RIP updates
    fn rip_go(slf_mutex: Arc<Mutex<Self>>) {
        loop {
            thread::sleep(Duration::from_secs(5));
            let slf = slf_mutex.lock().unwrap();
            slf.rip_broadcast();
        }
    }
    /// Periodically checks the entries of the forwarding table
    fn run_table_check(slf_mutex: Arc<Mutex<Self>>) {
        loop {
            thread::sleep(Duration::from_millis(12000));
            let slf = slf_mutex.lock().unwrap();
            let mut to_remove = Vec::new();
            //Thought it'd be easier just to loop through the forwarding table itself so that updating/deleting the route wouldn't be painful
            for (prefix, route) in &mut *slf.forwarding_table_mut() {
                match route.rtype {
                    RouteType::Rip if route.creation_time.elapsed().as_millis() >= 12000 => {
                        route.cost = Some(16);
                        to_remove.push(*prefix); //Clone used to avoid stinky borrowing issues
                    }
                    _ => {}
                }
            }
            if !to_remove.is_empty() {
                slf.rip_broadcast();
                to_remove.iter().for_each(|prf| {
                    slf.forwarding_table_mut()
                        .remove(prf)
                        .expect("Something fishy");
                });
            }
        }
    }

    ///BROADCAST FUNCTIONS

    ///Sends a RIP response to all RIP neighbors
    fn rip_broadcast(&self) {
        let keys: Vec<Ipv4Addr> = self.rip_neighbors().keys().cloned().collect(); // So tired of this ownership bullshit
        for addr in keys {
            self.rip_respond(addr, None);
        }
    }
    ///Sends a RIP response to a given destination IP containing routes correlating to the input option of
    ///a vector of Ipv4Nets - if None, send all routes
    fn rip_respond(&self, dst: Ipv4Addr, nets: Option<&Vec<Ipv4Net>>) {
        let rip_resp_msg: RipMsg = self.table_to_rip(nets);
        let pack = self.package_rmsg(rip_resp_msg, dst);
        self.send(pack);
    }
    fn table_to_rip(&self, nets: Option<&Vec<Ipv4Net>>) -> RipMsg {
        let mut rip_routes: Vec<RipRoute> = Vec::new();
        let forwarding_table = self.forwarding_table();
        let table = match nets {
            Some(nets) => {
                let mut ftable_subset = HashMap::new();
                nets.iter().for_each(|net| {
                    ftable_subset.insert(
                        net,
                        forwarding_table
                            .get(net)
                            .expect("Internal Failure: net should def be in the fwding table"),
                    );
                });
                ftable_subset
            }
            None => forwarding_table
                .iter()
                .collect(), //Weird ownership wizardry
        };
        for (mask, route) in table {
            match route.rtype {
                RouteType::ToSelf | RouteType::Static => continue,
                _ => {
                    let rip_route = RipRoute::new(
                        route.cost.unwrap(),
                        mask.clone().addr().into(),
                        mask.clone().netmask().into(),
                    );
                    rip_routes.push(rip_route);
                }
            }
        }
        RipMsg::new(2, rip_routes.len() as u16, rip_routes.to_vec())
    }

    //REQUEST FUNCTIONS

    ///Sends RIP requests to all RIP neighbors
    fn request_all(&self) {
        let keys: Vec<Ipv4Addr> = self.rip_neighbors().keys().cloned().collect(); // So tired of this ownership bullshit
        for addr in keys {
            self.rip_request(addr);
        }
    }
    ///Sends a RIP request to an input destination IP address
    fn rip_request(&self, dst: Ipv4Addr) {
        let rip_req_msg: RipMsg = RipMsg::new(1, 0, Vec::new());
        let pack = self.package_rmsg(rip_req_msg, dst);
        self.send(pack);
    }

    //UPDATE FUNCTIONS

    /// Updates a node's RIP table according to a RIP message - returns None if nothing gets changed
    fn update_fwd_table(&self, rip_msg: &mut RipMsg, next_hop: Ipv4Addr) -> Option<Vec<Ipv4Net>> {
        let mut updated = Vec::new();
        for route in &mut rip_msg.routes {
            if let Some(net) = route_update(route, self.forwarding_table_mut(), &next_hop) { updated.push(net) }
        }
        if !updated.is_empty() {
            Some(updated)
        } else {
            None
        }
    }
    fn triggered_update(&self, changed_routes: Option<Vec<Ipv4Net>>) {
        if let Some(changed_routes) = changed_routes {
            let keys: Vec<Ipv4Addr> = self.rip_neighbors().keys().cloned().collect(); // So tired of this ownership bullshit
            for addr in keys {
                self.rip_respond(addr, Some(&changed_routes));
            }
        }
    }

    //UTILITY FUNCTIONS

    ///Packages a RIP message in an IP Packet
    fn package_rmsg(&self, rmsg: RipMsg, dst: Ipv4Addr) -> Packet {
        let ser_resp_rip: Vec<u8> = serialize_rip(rmsg);
        let pb = PacketBasis {
            dst_ip: dst,
            prot_num: 200,
            msg: ser_resp_rip,
        };
        self.build(pb)
    }
}
