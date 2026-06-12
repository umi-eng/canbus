# PEAK-System USB CAN Driver

Async driver for the [PEAK System](https://www.peak-system.com/) PCAN-USB FD
adapter, built on top of [`nusb`].

Works with both `tokio` and `smol` async runtimes (default: `tokio`).

## Example

```rust,no_run
# async fn example() -> Result<(), pcan::Error> {
use pcan::{PCanFd, BitTiming, DataBitTiming, ChannelOptions, Frame};
use embedded_can::Frame as _;

let info = pcan::list_devices().await?
    .next()
    .expect("no PCAN-USB FD found");

let dev = PCanFd::open(info).await?;

// Classic CAN at 500 kbit/s
dev.open_channel(BitTiming::from_bitrate(500_000), ChannelOptions::default()).await?;

let frame = Frame::new(
    embedded_can::StandardId::new(0x123).unwrap(),
    &[1, 2, 3, 4],
).unwrap();
dev.send(frame).await?;

let msg = dev.recv().await?;
println!("{msg:?}");
# Ok(())
# }
```
