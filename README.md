# A pure Rust TCP Implementation over a simulated local network

Created for CSCI1680: Computer Networks
by William Stone and Alex Khosrowshahi (me)

---

## Prerequisites

- An updated Rust/Cargo installation
- tmux (for virtual network simulation)
- GNU make

---

## Compiling

1. Navigate to the top level directory (ordinarily named tcp/)
2. Run ```make``` to compile and build ```vhost``` and ```vrouter```
3. If errors arise, email me your frantic pleading in haiku form
[here](mailto:alexander_khosrowshahi@brown.edu)

---

## Running

The virtual network is full of dark and arcane magiks, beware...

1. Run ```util/vnet_run --router ./reference/vrouter --host ./vhost linear-r1h2```
2. Profit! See the below list for commands : )

> For those recruiters who may be prowling my github:
> We use the reference router because the TCP implementation is all host-focused!
> I do have a router implementation of my own, though.
> It should work fine (maybe, sorta)

---

## Commands

### c: Connect to a socket

Usage:

```bash
c <vip> <port>
```

Example:

```bash
c 10.0.0.1 9999
```

### a: Listen + Accept incoming connections

Usage:

```bash
a <port>
```

Example:

```bash
a 9999
```

### s: Send to a socket

Usage:

```bash
s <socket ID> <bytes>
```

Example:

```bash
s 0 hihihi
```

### r: Receive

Usage:

```bash
r <socket ID> <numbytes>
```

Example:

```bash
r 0 5
Read 5 bytes: hihih
```

### ls: List sockets

Example:

Here’s an example table with two sockets. Socket 0 is a listen socket,
socket 1 is a socket for a client that connected to this listen socket.

> | SID | LAddr    | LPort | RAddr    | RPort | Status      |
> |-----|----------|-------|----------|-------|-------------|
> | 0   | 0.0.0.0  | 9999  | 0.0.0.0  | 0     | LISTEN      |
> | 1   | 10.1.0.2 | 9999  | 10.0.0.1 | 46810 | ESTABLISHED |

### cl: Close socket

Usage:

```bash
cl <socket ID>
```

### sf: Send file (the meat)

Usage:

```bash
sf <file-path> <addr> <port>
```

Example:

```bash
sf path/to/some_file 10.1.0.2 9999

// Sends however many bytes in the specified file
```

### rf: Receive file (the potatoes)

Usage:

```bash
rf <dest file> <port>
```

Example:

```bash
rf path/to/some_destination_file 9999

[...]
Received (however many) bytes
```


