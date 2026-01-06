/* IP STACK API REQUIREMENTS

Initalization:
main will parse command args to find lnx file name, open lnx file, parse lnx file, and feed result to Initialize
pub fn initialize(configinfo: IpConfig) -> Result<IPDaemon> {}

Send Packets
SendIP(dst netip.Addr, protocolNum uint8, data []byte) (error)

Protocol Handler
type HandlerFunc func(...) // You decide what this function looks like
RegisterRecvHandler(protocolNum uint8, callbackFunc HandlerFunc)

*/

pub mod backends;
pub mod config;
mod conn_socket;
mod interface;
pub mod ip_daemons;
pub mod ip_handler; //b/c right now REPL makes IpHandler, although ideally this is a config task
mod prelude;
pub mod retransmission;
mod rip_trait;
pub mod rip_utils;
mod send_recv_utils;
pub mod socket_manager;
pub mod sockman_utils;
mod tcp_utils;
pub mod utils; //pub for testing purposes - should change back on deployment
pub mod vnode_traits;
