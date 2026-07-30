# GS USB CAN Driver

A host driver for working with USB-CAN interfaces that use the Geschwister Schneider protocol.

See [`usbd-gscan`](crates.io/crates/usbd-gscan) for a device-side implementation.

## Getting started

```rust,no_run
use embedded_can::Frame;
use embedded_can::StandardId;

#[tokio::main]
async fn main() -> Result<(), gsusb::Error> {
    let info = gsusb::list_devices().await?.next().expect("no devices");
    let device = gsusb::Device::open(info).await?;

    // start the interface channel
    device
        .start(
            0, // channel number
            gsusb::ChannelOptions {
                one_shot: true,
                loopback: true,
                ..Default::default()
            },
        )
        .await?;

    // set the bitrate
    device.bitrate(0, 500_000).await?;

    // send a frame
    device
        .send(
            0,
            &Frame::new(StandardId::new(0x7FF).unwrap(), &[1, 2, 3, 4]).unwrap(),
        )
        .await?;

    Ok(())
}
```
