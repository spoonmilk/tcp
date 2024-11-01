mod repl_trait;
mod repls;

use std::env;
use std::thread::spawn;
use std::sync::mpsc::channel;
use lnxparser::IPConfig;
use library::config::initialize;
use library::backends::Backend;

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
    let (backend, ip_recver) = initialize(config_info).expect("Error initializing backend");
    //Run REPL
    match backend {
        Backend::Host(hbackend) => {
            let repl = repls::HostRepl::new(hbackend);
            repl.run(ip_recver);
        }
        Backend::Router(rbackend) => {
            let repl = repls::RouterRepl::new(rbackend);
            repl.run(ip_recver);
        }
    }
}
