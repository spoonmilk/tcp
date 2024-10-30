use std::env;
use std::thread::spawn;
use std::sync::mpsc::channel;
use lnxparser::IPConfig;
use library::config::initialize;
mod router_repl;
//use library::ip_data_types::{IPDaemon, IPDaemonType};

fn main() {
    // Get command line args upon boot
    let args: Vec<String> = env::args().collect();
    // Take in file, verify --config was specified
    if &args[1] != "--config" {
        panic!("Config was not supplied or incorrectly supplied");
    }
    let file_path = (&args[2]).clone();
    //Initialize the IPDaemon
    let config_info: IPConfig = IPConfig::new(file_path);
    let nd = match initialize(config_info) {
        Ok(nd) => nd,
        Err(e) => panic!("Error initializing IPDaemon: {e:?}")
    };
    //Create a channel and run the IPDaemon
    let nd_type = nd.n_type.clone();
    let (send, recv) = channel();
    spawn(move || nd.run(recv));
    //Run REPL
    let _ = router_repl::run_repl(nd_type, send);
}
