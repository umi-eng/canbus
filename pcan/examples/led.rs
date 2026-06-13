use pcan::PCanFd;

async fn first_device() -> Option<nusb::DeviceInfo> {
    pcan::list_devices().await.ok()?.next()
}

#[tokio::main]
async fn main() {
    let info = first_device().await.expect("no device");
    let dev = PCanFd::open(info).await.expect("open failed");

    // blink the led quickly
    dev.set_led(pcan::LedMode::Fast)
        .await
        .expect("failed to set led");
}
