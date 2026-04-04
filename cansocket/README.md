# CANSocket

A safe Linux SocketCAN async wrapper with minimal dependencies. Utilises
`async-io` to be cross compatible with many executors and implements `embedded-can`
traits.

## Virtual CAN device for testing

Examples and integration tests require a virtual device `vcan0` to be created.

```sh
sudo ip link add dev vcan0 type vcan
sudo ip link set up vcan0
```
