# Design Outline


The underlying design idea that informed the way we went about structuring our code for this project was that of *autonomous objects*. We wanted our hosts, routers, and interfaces to be objects that ran themselves instead of being structs that had code executed on them. This may seem like a relatively minor semantic property of our program (and in many ways, it is), but this did entail some interesting consquences. With this design principle in mind, we focused our efforts around building two powerful structs capable of executing most of the important functions of the overall program: `Node` and `Interface`, both of which are stored in our IP library. `Interface` contains all of the fields and methods necessary to perform all the tasks interfaces need to do (somewhat obviously) while `Node` does the same for all the tasks a host or router would need to do (the field `n_type` informs `Node` whether or not it's a host or router, telling it to switch RIP on or off accordingly). This is probably somewhat standard across most programs people have made for this project, but there is one aspect of both of these objects that is perhaps somewhat different: both `Node` and `Interface` implement a `run` method that consumes `self` and, well, runs the `Node` or `Interface`. This means nearly all of the top level program logic is contained within these objects themselves; instead of there being a vhost and vrouter program, there is just one vnode program that simply runs the input lnx file through our initialize function (contained in our IP library in config.rs), spawns a thread that calls the output nodes `run` function, and then runs the REPL. When `make` is run for our project, we just `cargo build` the one vnode program into vrouter and then copy that binary and call the copy vhost.


A brief runthrough of the executing of our vhost/vrouter is as follows:
1. `initialize` is called on the input lnx file. This sets up and immediately runs all `Interface`s and configures before returning the `Node` the lnx file describes.
2. The program splits into two threads - one thread calls `run` on the `Node` output by `initialize` and one runs the REPL. The threads are connected by a channel by which the REPL communicates commands to the `Node`.
3. If the `Node` is a router, it sleeps for 0.1 second to insure all other routers are running before broadcasting sending RIP requests to all neighbors. `Node` also spawns two threads to help with RIP; one that broadcasts the `Node`s RIP routes once every 5 seconds and another that checks to see if routes have timed out on a periodic basis.
4. Regardless of whether the `Node` is a router, it begins running two threads - one listens for commands coming over the channel from the REPL and one listens for messages coming from the channels connecting each `Node` to each of its `Interface`s.
6. Meanwhile, each `Interface` is operating over two threads as well: one listening for incoming commands from its associated `Node` via channel and the other listening for UDP packets coming from other `Interfaces`.


# Abstractions Used


We used three layers of abstraction within this project, one for each of the relevant network layers present: an application layer, a network layer, and a link layer abstraction. The application layer abstraction is our REPL; the `run_repl` function abstracts away the functioning of the REPL from vhost/vrouter, the REPL making up our 'application' in this project. The network layer abstraction is our `Node` object; `Node::run()` abstracts away the packet forwarding and interface managing functioning from vhost/vrouter. Finally, the link layer abstraction is our `Interface` object; `Interface::run()` abstracts away the sending of packets over UDP between routers and hosts from vhost/vrouter. The interactions between these layers is managed via the use of channels; if the application layer needs to send data across the network, it lets the network layer (`Node`) know this by sending it a command over a channel, which performs some logic before commanding the link layer (`Interface`) over a channel to take care of the actual sending. One final abstraction is the `initialize` function, which abstracts away the generation of `Interface`s and `Node`s from vhost/vrouter.


# Steps Taken to Process IP Packets


1. Packet is received over UDP by `Interface`
2. IP header is deserialized (its payload is not) and the deserialized header plus the packet's payload is packaged in a `Packet` object
3. This `Packet` is passed up to `Interface`'s `Node` via channel
4. The `Packet` is checked for validity - ttl is greater than 0 and checksum is valid
5. The `Packet` is updated - ttl decremented and checksum recalculated
6. The `Packet` is run through the forwarding table until it bottoms out - longest prefix matching finds the proper route and if that route points to an IP address, the process repeats but with that IP address
7. (a) If forwarding bottomed out on an `Interface`, the packet is passed via channel to that `Interface` with information about the `Packet`'s next hop (proceed to 8.)
7. (b) If forwarding bottomed out on a `ToSelf` route, the packet's payload is processed and handled accordingly (either printed to stdout or handled as a RIP message)
8. The `Packet` is serialized into bytes to send it
9. The serialized packet is sent to the appropriate `Interface` across the network over UDP


# Annotated File Structure


/ip-the-better-tech-house-group
| - /ip-imp //Rust workspace for the project
| | - /library/src
| | | - config.rs //Contains the `intialize` function used to turn a lnx file into a `Node`
| | | - ip_data_types.rs //Contains `Node` code and datatypes that need to be publicly accessible
| | | - lib.rs //Unifies all files in /library/src into a library crate
| | | - prelude.rs //Contains most imports used by library files
| | | - rip_utils.rs //Contains several utility functions relevant to RIP that `Node` uses
| | | - utils.rs //Contains `Interface` code and datatypes that don't need to be publicly accessible
| | - /lnxparser //Cloned from IP project utils github
| | - /vnode/src
| | | - main.rs //Code that compiles to vhost and vrouter
| | | - repl.rs //Provides `run_repl` function - main thread of vhost and vrouter runs this
| - Makefile //Compiles ip-imp/vnode/src/main.rs to vhost and vrouter
| - vhost
| - vrouter


# Bugs/Potential Improvements


At the present moment, we are unaware of any logical/functional bugs in our program. We have tested our implementation on several network topologies, turning `Interface`s on and off and ensuring that our packets are sent across the most cost effective path (if possible) and are properly handled at their destination, and at the present, all we have seen looks correct.
There is, however, at least one improvement we would add if we had the energy/time to do so. At the moment, our `Node` prints messages directly to stdout when it handles errors, receives test packets, and when prompted to list information by the REPL. This throws off our REPL prompt, as the prompt gets written over by these messages and also stands in violation to our program's structure; in theory, the application layer should be handling all interactions with the user, so the application layer ought to be responsible for printing messages of all kinds (except perhaps panic messages caused by internal failure). Adding a channel from `Node` to the REPL to pass along messages that need printing to the REPL would fix this breach of design and would allow us to reprint the prompt after messages get printed, solving this issue. We have deemed the effort to add this, though, to be unwarranted simply to fix a small user-facing issue in this mostly internal-facing project, which is why steps haven't been taken to fix this.
