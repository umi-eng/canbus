use embedded_can::Frame;
use embedded_can::StandardId;
use gsusb::ChannelOptions;
use gsusb::Error;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let info = gsusb::list_devices().await?.next().expect("no devices");
    let device = gsusb::Device::open(info).await?;

    device
        .start(
            0,
            ChannelOptions {
                one_shot: true,
                loopback: true,
                ..Default::default()
            },
        )
        .await?;

    println!("Set bitrate to 500kbit/s");
    device.bitrate(0, 500_000).await?;

    loop {
        println!("Sending a frame");
        device
            .send(
                0,
                &Frame::new(StandardId::new(0x7FF).unwrap(), &[1, 2, 3, 4]).unwrap(),
            )
            .await?;

        sleep(Duration::from_secs(1)).await;
    }
}
