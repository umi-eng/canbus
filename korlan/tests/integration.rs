use embedded_can::Frame as _;
use korlan::{BitTiming, CanMessage, ChannelOptions, Device, Frame, list_devices};
use nusb::DeviceInfo;
use std::time::Duration;

/// Serialize tests to prevent multiple access to hardware device.
static HW_LOCK: std::sync::LazyLock<std::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(()));

fn first_device() -> Option<DeviceInfo> {
    list_devices().ok()?.next()
}

#[tokio::test]
async fn test_list() {
    let _guard = HW_LOCK.lock().unwrap();

    let devices: Vec<_> = list_devices().expect("list_devices failed").collect();
    assert!(!devices.is_empty(), "no Korlan device found");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_open_and_version() {
    let _guard = HW_LOCK.lock().unwrap();

    let info = first_device().expect("no device");
    let dev = Device::open(info).await.expect("open failed");
    let ver = dev.version().await.expect("version failed");
    println!(
        "Firmware {}.{}, Hardware {}.{}",
        ver.fw_major, ver.fw_minor, ver.hw_major, ver.hw_minor
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_set_bitrate() {
    let _guard = HW_LOCK.lock().unwrap();

    let info = first_device().expect("no device");
    let dev = Device::open(info).await.expect("open failed");
    dev.set_bitrate(500_000).await.expect("set_bitrate failed");
    dev.close_channel().await.expect("close failed");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_loopback_send_recv() {
    let _guard = HW_LOCK.lock().unwrap();

    let info = first_device().expect("no device");
    let dev = Device::open(info).await.expect("open failed");

    dev.open_with(
        BitTiming::from_bitrate(500_000),
        ChannelOptions {
            loopback: true,
            ..Default::default()
        },
    )
    .await
    .expect("open_with failed");

    let sent = Frame {
        id: embedded_can::Id::Standard(embedded_can::StandardId::new(0x42).unwrap()),
        data: [1, 2, 3, 4, 0, 0, 0, 0],
        dlc: 4,
        rtr: false,
    };
    dev.send(sent.clone()).await.expect("send failed");

    let msg = tokio::time::timeout(Duration::from_secs(2), dev.recv())
        .await
        .expect("recv timed out")
        .expect("recv error");

    match msg {
        CanMessage::Frame(f) => {
            assert_eq!(f.id, sent.id);
            assert_eq!(f.dlc, sent.dlc);
            assert_eq!(&f.data[..4], &sent.data[..4]);
        }
        CanMessage::Error(e) => panic!("got error frame: {e:?}"),
    }

    dev.close_channel().await.expect("close failed");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_extended_frame_loopback() {
    let _guard = HW_LOCK.lock().unwrap();

    let info = first_device().expect("no device");
    let dev = Device::open(info).await.expect("open failed");

    dev.open_with(
        BitTiming::from_bitrate(250_000),
        ChannelOptions {
            loopback: true,
            ..Default::default()
        },
    )
    .await
    .expect("open_with failed");

    let sent = Frame {
        id: embedded_can::Id::Extended(embedded_can::ExtendedId::new(0x1234_5678).unwrap()),
        data: [0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0x00, 0x01],
        dlc: 8,
        rtr: false,
    };
    dev.send(sent.clone()).await.expect("send failed");

    let msg = tokio::time::timeout(Duration::from_secs(2), dev.recv())
        .await
        .expect("recv timed out")
        .expect("recv error");

    match msg {
        CanMessage::Frame(f) => {
            assert_eq!(f.id, sent.id);
            assert_eq!(f.data, sent.data);
            assert!(f.is_extended());
        }
        CanMessage::Error(e) => panic!("got error: {e:?}"),
    }

    dev.close_channel().await.expect("close failed");
}
