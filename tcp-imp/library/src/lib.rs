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

mod prelude;
mod interface;
pub mod utils; //pub for testing purposes - should change back on deployment
mod rip_trait;
mod tcp_utils;
mod conn_socket;
pub mod sockman_utils;
pub mod ip_handler; //b/c right now REPL makes IpHandler, although ideally this is a config task
pub mod vnode_traits;
pub mod config;
pub mod backends;
pub mod ip_daemons;
pub mod socket_manager;
pub mod rip_utils;