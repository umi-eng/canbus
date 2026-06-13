use pcan::BitTiming;
use pcan::ChannelOptions;
use pcan::PCanFd;

/// Serialise tests so they don't race on the single USB device.
static HW_LOCK: std::sync::LazyLock<std::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(()));

async fn first_device() -> Option<nusb::DeviceInfo> {
    pcan::list_devices().await.ok()?.next()
}

async fn all_devices() -> Vec<nusb::DeviceInfo> {
    pcan::list_devices()
        .await
        .ok()
        .map(|it| it.collect())
        .unwrap_or_default()
}

/// Enumerate and print all found PCAN-USB FD adapters.
///
/// Use `--no-capture` to show in test logs.
#[tokio::test]
async fn test_list_devices() {
    let _g = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let devices = all_devices().await;
    assert!(
        !devices.is_empty(),
        "no PCAN-USB FD device found - is the adapter plugged in?"
    );
    for d in &devices {
        println!(
            "  {:04x}:{:04x} – {}",
            d.vendor_id(),
            d.product_id(),
            d.product_string().unwrap_or("<no name>")
        );
    }
}

/// Open the device and verify the firmware info is populated.
///
/// Use `--no-capture` to show in test logs.
#[tokio::test]
async fn test_firmware_info() {
    let _g = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let info = first_device()
        .await
        .expect("no PCAN-USB FD device found - is the adapter plugged in?");

    let dev = PCanFd::open(info).await.expect("Device::open failed");
    let fw = &dev.fw_info;
    println!(
        "HW v{}, FW v{}.{}.{}, serial 0x{:08X}",
        fw.hardware_version,
        fw.firmware_version[0],
        fw.firmware_version[1],
        fw.firmware_version[2],
        fw.serial_number
    );
    assert_ne!(
        fw.serial_number, 0,
        "serial should not be zero for a real device"
    );
}

/// Open the channel at 500 kbit/s (classic CAN) and close it.
#[tokio::test]
async fn test_open_close_500k() {
    let _g = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let info = first_device()
        .await
        .expect("no PCAN-USB FD device - is the adapter plugged in?");

    let dev = PCanFd::open(info).await.expect("open failed");
    dev.set_bitrate(500_000).await.expect("set_bitrate failed");
    dev.close_channel().await.expect("close_channel failed");
}

/// Repeat open/close at all standard CAN bitrates to verify bittiming is
/// accepted by the hardware.
#[tokio::test]
async fn test_open_close_all_bitrates() {
    let _g = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let info = first_device()
        .await
        .expect("no PCAN-USB FD device - is the adapter plugged in?");

    let dev = PCanFd::open(info).await.expect("open failed");

    for &bitrate in &[
        10_000u32, 20_000, 50_000, 100_000, 125_000, 250_000, 500_000, 1_000_000,
    ] {
        dev.set_bitrate(bitrate)
            .await
            .unwrap_or_else(|e| panic!("set_bitrate({bitrate}) failed: {e}"));
        dev.close_channel()
            .await
            .unwrap_or_else(|e| panic!("close_channel after {bitrate} failed: {e}"));
    }
}

/// Open CAN-FD channel at 500k nominal / 2M data and close it.
#[tokio::test]
async fn test_open_close_canfd() {
    let _g = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let info = first_device()
        .await
        .expect("no PCAN-USB FD device - is the adapter plugged in?");

    let dev = PCanFd::open(info).await.expect("open failed");
    dev.open_channel(BitTiming::from_bitrate(500_000), ChannelOptions::default())
        .await
        .expect("open_channel failed");
    dev.set_data_bitrate(2_000_000)
        .await
        .expect("set_data_bitrate failed");
    dev.close_channel().await.expect("close_channel failed");
}

/// Open in listen-only mode (confirmed by command succeeding without error).
#[tokio::test]
async fn test_listen_only_mode() {
    let _g = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let info = first_device()
        .await
        .expect("no PCAN-USB FD device - is the adapter plugged in?");

    let dev = PCanFd::open(info).await.expect("open failed");
    dev.open_channel(
        BitTiming::from_bitrate(500_000),
        ChannelOptions {
            listen_only: true,
            ..Default::default()
        },
    )
    .await
    .expect("open_channel (listen_only) failed");
    dev.close_channel().await.expect("close failed");
}
