use crate::prelude::*;
use crate::utils::*;
use crate::vnode_traits::*;

//I'm thinking that initialize() will now return a Backend, so it'll need this
pub enum  Backend {
    Host(HostBackend),
    Router(RouterBackend)
}

pub struct HostBackend {
    interface_reps: Arc<RwLock<InterfaceTable>>,
    forwarding_table: Arc<RwLock<ForwardingTable>>,
    ip_sender: Sender<PacketBasis>
    //socket_table - coming soon
}

impl VnodeBackend for HostBackend {
    fn interface_reps(&self) -> RwLockReadGuard<InterfaceTable> { self.interface_reps.read().unwrap() }
    fn interface_reps_mut(&self) -> RwLockWriteGuard<InterfaceTable> { self.interface_reps.write().unwrap() }
    fn forwarding_table(&self) -> RwLockReadGuard<ForwardingTable> { self.forwarding_table.read().unwrap() }
    fn ip_sender(&self) -> &Sender<PacketBasis> { &self.ip_sender }
}

impl HostBackend {
    pub fn new(interface_reps: Arc<RwLock<InterfaceTable>>, forwarding_table: Arc<RwLock<ForwardingTable>>, ip_sender: Sender<PacketBasis>) -> HostBackend {
        HostBackend { interface_reps, forwarding_table, ip_sender }
    }
    pub fn tcp_send(&self, pb: PacketBasis) -> () {} //COMING SOON
}

pub struct RouterBackend {
    interface_reps: Arc<RwLock<InterfaceTable>>,
    forwarding_table: Arc<RwLock<ForwardingTable>>,
    ip_sender: Sender<PacketBasis>,
    //NO SOCKET TABLE NEEDED
}

impl VnodeBackend for RouterBackend {
    fn interface_reps(&self) -> RwLockReadGuard<InterfaceTable> { self.interface_reps.read().unwrap() }
    fn interface_reps_mut(&self) -> RwLockWriteGuard<InterfaceTable> { self.interface_reps.write().unwrap() }
    fn forwarding_table(&self) -> RwLockReadGuard<ForwardingTable> { self.forwarding_table.read().unwrap() }
    fn ip_sender(&self) -> &Sender<PacketBasis> { &self.ip_sender }
}

impl RouterBackend {
    pub fn new(interface_reps: Arc<RwLock<InterfaceTable>>, forwarding_table: Arc<RwLock<ForwardingTable>>, ip_sender: Sender<PacketBasis>) -> RouterBackend {
        RouterBackend { interface_reps, forwarding_table, ip_sender }
    }
}