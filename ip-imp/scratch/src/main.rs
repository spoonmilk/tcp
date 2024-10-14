use library::ip_data_types::{Node, NodeType};
use std::collections::HashMap;
use tokio::sync::mpsc::channel;
fn main() {
    let mut nd = Node::new(NodeType::Host, vec![], HashMap::new());
    let (_, recv) = channel(32);
    nd.run(recv);
}
