# Driver for the 8Devices Korlan USB2CAN

The [Korlan USB2CAN](https://www.8devices.com/products/korlan) is a galvanically
isolated CAN interface capable of up to 2 MBit/s bitrates.

# Example

```no_run
# async fn run() -> Result<(), korlan::Error> {
let info = korlan::list_devices()?.next().unwrap();
let device = korlan::Device::open(info).await?;
let ver = device.version().await?;
println!("fw {}.{} hw {}.{}", ver.fw_major, ver.fw_minor, ver.hw_major, ver.hw_minor);

device.open_with(korlan::BitTiming::from_bitrate(500_000), korlan::ChannelOptions::default()).await?;
device.send(korlan::Frame { id: 0x123, data: [1,2,3,4,0,0,0,0], dlc: 4, ext: false, rtr: false }).await?;
let msg = device.recv().await?;
println!("{:?}", msg);
device.close_channel().await?;
# Ok(()) }
```
