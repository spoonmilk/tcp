# Project III: TCP - William Stone & Alex Khosrowshahi

## Understanding our TCP Implementation

### The Socket Manager
The "Socket Manager" is our mechanism for handling the functionality of
listener sockets. Instead of having separate listener socket objects created
every time listen() is called, all calls to listen() and accept() are routed to
a singular socket manager. This socket manager object then has an internal
table of listening sockets that keeps track of all the state data relevant to
each one; listen() adds to this table and accept() flips a boolean representing
whether or not a given listener socket is currently accepting in the
corresponding entry on this table. Listener sockets are still given entries in
the socket table like usual but any packets destined for a listener socket are
routed to the socket manager to be dealt with. 

### What are the key data structures that represent a connection?

#### Connection Socket Send/Receive Buffers 
The Send and Receive buffers, like the socket manager, mark another point where
our implementation diverges from the typical one. 
These buffers are composed of two layers of wrapping around a circular buffer;
the inner layer of wrapping contains 
state related to the data stored in the buffer and methods for
extracting/adding data to them in an orderly fashion 
and the outer layer of wrapping contained a condition variable used to indicate
when important conditions regarding 
the buffers are fulfilled (eg. being full or empty). The design of these
wrapping layers (and the reason why they 
perhaps diverge from typical implemenations) was largely predicated by the way
the circular buffers we used 
(imported from the circular_buffer crate) were designed. Typical circular
buffers 
(at least from what we gathered from lecture and gear up slides) are
essentially wrappers around 
arrays of a capacity set at initialization. Circular buffers from this crate,
however, 
act more like dequeues with a set maximum capacity (after which newly added
elements replace elements in the front of the dequeue). 
In practice, we never used the circular nature of these circular buffers and
instead relied on their dequeue-esque functionality;
anytime we no longer needed data stored towards the front of a buffer, we were
able to simply drain it. 
This meant that we didn't need to keep track of a una or lbw pointer for the
send buffer, as una would always equal 0 and lbw would always equal
circ_buffer.len() (equal to the amount of data currently in the circular
buffer, not its overall capacity) and didn't need to keep track of either lbr
or nxt pointers in the receive buffer for the same reason. Furthermore, this
structure meant it made more sense to store data that might be needed for a
retransmission (on the sending side) and early arrivals (on the receiving side)
in separate data structures. At first, this atypical way of structuring these
buffers seemed to simplify things (significantly less pointers to keep track of
was nice) and, we believe, actually lead to a more efficient way of dealing
with early arrivals via a separate hashmap, but came at the cost of making
retransmissions and ZWP significantly more cumbersome. If we were to do this
project all over again, we would probably opt to implement via the more typical
approach to circular buffers (possibly implementing one ourselves); we believe
this different way of doing things was more trouble than it was worth.


#### Retransmission Buffers

Our retransmission buffers use a fairly simple queue structure,
using the builtin VecDeque data structure from the standard library.
Additionally, we have a self-defined structure of retransmission segments
that store essentially the data of a built packet so that we don't have
to deal with checksum weirdness when retransmitting. 

We also have a retransmissiontimer struct, which interacts with the time_wait
thread and ack logic in the queue for resets and rtt calculations.

### High level thread logic overview
Unlike our implementation of interfaces and the IP layer from the IP part of
the project, our connection sockets do not have a thread or a collection of
threads constantly running to take care of their functionality. Instead,
threads are created to execute their various functions when needed. There are
two types of occurrences that cause this to happen: a packet destined for a
connection socket is received or a socket is ordered to send, receive, or close
by the REPL. In the first case, a thread is created to handle the reception of
the packet which never has to wait on anything besides acquiring read/write
locks for the send and receive buffers on occasion and terminates in a
predictable manner. In the latter case, the threads with more complicated
behavior are spawned. In the case of send(), the thread created to handle
sending spawns a partner thread (which we refer to as the send_onwards thread)
and the original thread does not exit until it has succesfully joined this
partner thread. While sending, the original sending thread loops through
filling the send buffer with data (and waiting on a condition variable to
indicate that space has opened up in the send buffer, a notification that the
packet reception thread is responsible for when it "acks" data from the send
buffer) while the send_onwards thread loops through actually sending the data
within the send buffer outwards in packets (and waiting on a channel to
indicate when the send buffer is not empty, a notification the sending thread
is responsible for). When ZWP occurs, the send_onwards thread grinds to a halt
and waits on a channel to tell it that ZWP is over while spawning another
thread to sending probe packets. In the case of receive(), the thread
responsible for reception waits on a condition variable (alerted by the packet
reception thread when packets are received) which lets it know that data is
available in the receive buffer. And in the case of close() the created thread
waits for all sending to complete before finishing execution.
For retransmission, we spawn a thread running the time_wait() function,
which waits for the calculated rto period before waking up and checking for 
segments to be retransmitted.
The final piece of the puzzle is a constantly running
thread external to the connection sockets that waits on a channel for messages
that a given connection socket is entering the CLOSED state; upon receiving a
message on this channel, the thread deletes the socket table entry for said
connection socket before waiting on this channel again.

### Oh, what we wish we could have done

We would have liked to implement a version using async/await functions, which
actually could have remedied some of our issues potentially, but we found
dealing with the complexities of async Rust a bit too demanding for this project.

We also went through an extremely intense period of refactoring, which was
mostly due to incompatibilities with the spec for TCP on our IP layer.
Given how much time we lost on these, it would have been nice to have done
more refactoring earlier.

### Known bugs/issues
As of the writing of this README, we have only recognized one bug in our
implementation: a fairly regular occurrence of duplicate acks/window updates
coming from our receiver when they shouldn't be appearring (they still occur
even when there is no packet dropping). We have looked into this bug thoroughly
and uncovered some of the underlying issue: weirdly enough, even while packets
appear to be getting sent in order, they appear to be *actually getting
processed by our sockets* out of order. The nice thing about this bug, though,
is at the end of the day, all sent packets are received, so this doesn't seem
to be a major deal as far as functionality is concerned beyond perhaps a loss
in efficiency. Anyways, this bug is a bit of a head scratcher since we can't
think of any TCP implementation reason why this might happen; all we know is
that the packets are getting received out of order by the sockets on occasion.
We now believe that the issue likely lies somewhere in our IP implementation,
although we are also uncertain as to what it may be; the IP layer should
immediately pass up packets when it receives them. We decided to stop our
debugging process there because of time constraints and a desire not to have to
go back and deal with faulty IP code.

On consequence of the duplicate acks/window updates is mislabeled spurious retransmissions.
Because some window updates are being transmitted that are actually dup acks (and vice versa), 
we have extremely frequent fast retransmission triggers which are then caught by
wireshark as spurious despite technically being correct.

Another issue that points to our bugs stemming from the IP layer is the weird behavior
that happens with low RTO values. Things get sent out of order, retransmitted strangely,
and just generally don't function as they should, but are immediately behaving differently
with higher RTO values. Along with our performance, this leads us to believe
these issues are at their core on the IP layer.

## Performance Measurement and Capture

### Measuring Performance

Our performance measurement capture is in the effiency-capture file. While
we don't run as fast as the reference, we believe a lot of this is due to the 
sending of excessive duplicate ACKs which increases packet overhead and inefficiency
in our IP layer, which was shown to be much slower than reference during our IP grading meeting. 
We've also included a capture of the reference sending a 1MB file, titled
reference-comparison, in which you can observe these differences.

### Packet capture annotations
We've included a packet capture in our directory under the title "normal-test-capture."
This includes all the things we've noted. 

#### Annotated requirements

- Our three-way handshake happens on frames 1, 2, and 3 of our packet capture, executing successfully before data begins sending. 
- In frame 17, we see a segment sent with seq=11041, which is acknowledged in frame 21 by h2.
- In frame 1223, you can see a normal fast retransmission after duplicate ACKs.
- In frames 1530-1543, we see the normal closing sequence. Hooray!
