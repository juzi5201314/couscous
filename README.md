# couscous

A remote port mapping tool, similar to `frp`, which maps a specified local port (after nat) to a remote server.

This project is for experimenting with ideas and using it myself, so it's full of instability and bugs. If you're looking for a good alternative to frp, [rathole](https://github.com/rapiz1/rathole) is a very good project.

## todo
* Multiple connections to transfer data (connection pooling)
* A connection that controls other information (such as keep-alive, mapping information, etc.) independent of the data transmission channel
* Tests
* Test the transmission on a bad network and how to optimize it.

## data transmission method
quic

## build
### native build
```shell
RUSTFLAGS="-Ctarget-cpu=native" cargo build --release --features=native
```

## benchmark
### udp (by iperf3)
`iperf3 -c 127.0.0.1 -p {port} -u -b 0`

#### couscous(native build) #56436ca346f5f21403c2e633a8ac05e129bfc264
```
[ ID] Interval           Transfer     Bitrate         Jitter    Lost/Total Datagrams
[  5]   0.00-10.00  sec  4.16 GBytes  3.57 Gbits/sec  0.000 ms  0/3081710 (0%)  sender
[  5]   0.00-10.01  sec   951 MBytes   797 Mbits/sec  0.007 ms  2391007/3079850 (78%)  receiver
````

#### frp 0.38.0
```
[ ID] Interval           Transfer     Bitrate         Jitter    Lost/Total Datagrams
[  5]   0.00-10.00  sec  3.13 GBytes  2.69 Gbits/sec  0.000 ms  0/2324420 (0%)  sender
[  5]   0.00-10.00  sec   367 MBytes   307 Mbits/sec  0.059 ms  2049996/2315460 (89%)  receiver
```
