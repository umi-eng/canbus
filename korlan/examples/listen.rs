#[tokio::main]
async fn main() {
    let info = korlan::list_devices()
        .expect("List devices")
        .next()
        .expect("First device");

    let device = korlan::Device::open(info).await.expect("Open device");

    device.set_bitrate(500_000).await.expect("Set bitrate");

    loop {
        match device.recv().await {
            Ok(korlan::CanMessage::Frame(frame)) => display_frame(&frame),
            Ok(korlan::CanMessage::Error(error)) => println!("Bus error: {:?}", error),
            Err(err) => eprintln!("Error: {:?}", err),
        }
    }
}

/// Prints frames similar to the `candump` tool.
fn display_frame(frame: &korlan::Frame) {
    match frame.id {
        embedded_can::Id::Extended(id) => print!("{:08X} ", id.as_raw()),
        embedded_can::Id::Standard(id) => print!("{:03X} ", id.as_raw()),
    }

    print!("[{}] ", frame.dlc);

    for b in frame.data {
        print!("{:02X} ", b);
    }

    println!();
}
