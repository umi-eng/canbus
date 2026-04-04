use cansocket::{Error, Frame};

/// Serialize tests to prevent multiple access to vcan device.
static LOCK: std::sync::LazyLock<std::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(()));

#[test]
fn test_open_missing_device() {
    assert!(cansocket::Socket::new("nonexistant").is_err());
}

#[test]
fn test_open_vcan() {
    let _vcan = LOCK.lock().unwrap();
    assert!(cansocket::Socket::new("vcan0").is_ok());
}

#[test]
fn test_recv_own_msgs_blocking() -> Result<(), Error> {
    let _vcan = LOCK.lock().unwrap();

    let socket = cansocket::Socket::new("vcan0")?;
    socket.recv_own_msgs(true)?;

    let id = embedded_can::StandardId::new(100).unwrap();
    let frame = cansocket::Frame::new(id, &[4, 5, 6, 7]).unwrap();

    socket.send_blocking(&frame)?;
    assert_eq!(socket.recv_blocking()?, frame);

    Ok(())
}

#[tokio::test]
async fn test_recv_own_msgs_async() -> Result<(), Error> {
    let _vcan = LOCK.lock().unwrap();

    let socket = cansocket::Socket::new("vcan0")?;
    socket.recv_own_msgs(true)?;

    let id = embedded_can::StandardId::new(101).unwrap();
    let frame = cansocket::Frame::new(id, &[4, 5, 6, 7]).unwrap();

    socket.send(&frame).await?;
    assert_eq!(socket.recv().await?, frame);

    Ok(())
}

#[test]
fn test_loopback_blocking() -> Result<(), Error> {
    let _vcan = LOCK.lock().unwrap();

    let socket1 = cansocket::Socket::new("vcan0")?;
    socket1.loopback(true)?;
    // loopback is enabled by default
    let socket2 = cansocket::Socket::new("vcan0")?;
    socket2.loopback(false)?; // receiver shouldn't need loopback

    let id = embedded_can::StandardId::new(102).unwrap();
    let frame = cansocket::Frame::new(id, &[4, 5, 6, 7]).unwrap();

    socket1.send_blocking(&frame)?;
    assert_eq!(socket2.recv_blocking()?, frame);

    Ok(())
}

#[tokio::test]
async fn test_loopback_async() -> Result<(), Error> {
    let _vcan = LOCK.lock().unwrap();

    let socket1 = cansocket::Socket::new("vcan0")?;
    socket1.loopback(true)?;
    // loopback is enabled by default
    let socket2 = cansocket::Socket::new("vcan0")?;
    socket2.loopback(false)?; // receiver shouldn't need loopback

    let id = embedded_can::StandardId::new(102).unwrap();
    let frame = cansocket::Frame::new(id, &[4, 5, 6, 7]).unwrap();

    socket1.send(&frame).await?;
    assert_eq!(socket2.recv().await?, frame);

    Ok(())
}

#[test]
fn test_recv_protocol_error() -> Result<(), Error> {
    let _vcan = LOCK.lock().unwrap();

    let socket = cansocket::Socket::new("vcan0")?;
    socket.protocol_errors(true)?;
    socket.recv_own_msgs(true)?;

    let mut frame: libc::can_frame = unsafe { std::mem::zeroed() };
    frame.can_id = libc::CAN_ERR_FLAG;
    frame.can_id |= libc::CAN_ERR_ACK;
    socket.send_blocking(&Frame::from(frame))?;
    assert!(socket.recv_blocking().is_err());

    Ok(())
}
