#[path = "ip_data_types.rs"]
mod ip_data_types;

pub mod config {
    /// Handles initializing routers, returns to initialize
    fn init_router(config_info: IPConfig) -> Result<Node> {}

    /// Handles initializing hosts, returns to intialize
    fn init_host(config_info: IPConfig) -> Result<Node> {} 

    /// Takes in an IPConfig from parsing, returns a Result with the Node on success.
    pub fn initialize(config_info: IPConfig) -> Result<Node> {} 

}