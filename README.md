# couscous

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
