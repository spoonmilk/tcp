
**Relevant Links:**

[https://brown-csci1680.github.io/iptcp-docs/#core-requirements](https://brown-csci1680.github.io/iptcp-docs/#core-requirements)

**Questions to consider:**

**What are the different parts of your IP stack and what data structures do they use? How do these parts interact (API functions, channels, shared data, etc.)?**

<span style="text-decoration:underline;">Interface struct:</span>

Name: &str

Virtual IP: std::net::Ipv4Addr

UDP Socket: std::net::UdpSocket

Neighbors: HashSet&lt;IpAddr>

Note from Nick: Interfaces are discrete from nodes, should be aware of their neighbors independently

<span style="text-decoration:underline;">Nodes (routers, hosts):</span>

Interfaces: Vec&lt;Interface>

Hashmap&lt;Netmask, Option&lt;IpAddr, Interface>> for forwarding table

<span style="text-decoration:underline;">Three main methods of interaction:</span>



* Interface -> Node - An interface reports data received from another interface to its parent node - via channel
* Node -> Interface - A node commands an interface to send data to another interface - via channel
* Interface -> Interface - An interface sends a packet to an interface attached to a different node - over UDP (likely encapsulated in an API call)

**What fields in the IP packet are read to determine how to forward a packet?**

The fields relevant to determine how a packet should be handled upon reception are its destination IP address and its TTL field. If the TTL is 0, the packet gets dropped and no further forwarding occurs. If not, the destination IP address goes through the forwarding table of whatever node the packet was received in; if the IP address matches with node’s own IP address, packet forwarding stops and the packet is handed over to some internal function that handles its contents and if the IP address matches on some other prefix, it is sent off via whatever interface the table says is responsible for it. While the destination MAC address is used within the link layer to determine which interface should pick up the packet, it is abstracted away from the forwarding process and thus not read as a part of it.

**What will you do with a packet destined for local delivery (ie, destination IP == your node’s IP)?**

There will be an entry in the IP forwarding hashmap that is a 32 bit prefix that matches a given node’s IP address completely. When the destination IP address of a received packet matches with this prefix, the packet will get handed over to some internal node function, which will parse it based on its protocol type. If it is of protocol type 0, for instance, its packet’s contents will be printed to stdout and if it is of protocol type 200, it will be parsed as a RIP packet and the node’s forwarding table will be updated accordingly. 

**What structures will you use to store routing/forwarding information?**

We plan on using a HashMap with netmasks as keys and an enum of either an interface, an IP address, or some fieldless type as values. Entries with an interface as value will route packets to those interfaces for forwarding, entries with an IP address as value will run the packet through the forwarding table again, this time matching on that IP address, and entries with the fieldless type as value will pass packets to an internal function to be processed (ie, these packets were destined for the node they just arrived at).

**What happens when a link is disabled? (ie, how is forwarding affected)?**

RIP should take care of disabled links. Nodes attached to a recently disabled link will timeout their connections with the node across the disabled link and will edit their forwarding tables and send RIP messages to their other connected nodes accordingly. This ripple effect of alerts will propagate across the network until all nodes have altered their forwarding tables to take into account the severance. Packets sent along a route that assumed the functionality of the disabled link will either find themselves redirected as nodes alter their forwarding tables or will be dropped, in which case it is the burden of the communicating parties, not the network, to handle the missing packets. 

