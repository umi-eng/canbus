use embedded_can::{Frame, StandardId};
use gsusb::{ChannelOptions, Error};

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

    println!("Sending a frame");
    device
        .send(
            0,
            &Frame::new(StandardId::new(123).unwrap(), &[1, 2, 3, 4]).unwrap(),
        )
        .await?;

    println!("Receiving a frame");
    println!("Received: {:?}", device.recv().await?);

    Ok(())
}
