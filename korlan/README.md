# Driver for the 8Devices Korlan USB2CAN

The [Korlan USB2CAN](https://www.8devices.com/products/korlan) is a galvanically
isolated CAN interface capable of up to 2 MBit/s bitrates. This crate provides
an async userspace driver built upon the pure-Rust
[`nusb`](https://github.com/kevinmehall/nusb) which supports either
[`tokio`](https://github.com/tokio-rs/tokio) or
[`smol`](https://github.com/smol-rs/smol) as the runtime.

# Example

```rust
# async fn run() -> Result<(), korlan::Error> {
# use embedded_can::{Id, StandardId, ExtendedId};
let info = korlan::list_devices().await?.next().unwrap();
let device = korlan::Device::open(info).await?;
let ver = device.version().await?;
println!("fw {}.{} hw {}.{}", ver.fw_major, ver.fw_minor, ver.hw_major, ver.hw_minor);

device.open_with(korlan::BitTiming::from_bitrate(500_000), korlan::ChannelOptions::default()).await?;
let id = embedded_can::Id::Standard(embedded_can::StandardId::new(0x123).unwrap());
device.send(korlan::Frame { id, data: [1,2,3,4,0,0,0,0], dlc: 4, rtr: false }).await?;
let msg = device.recv().await?;
println!("{:?}", msg);
device.close_channel().await?;
# Ok(()) }
```
