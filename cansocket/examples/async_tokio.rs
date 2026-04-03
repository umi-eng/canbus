// To run this example, create a vcan device:
// sudo ip link add dev vcan0 type vcan
// sudo ip link set up vcan0

#[tokio::main]
async fn main() -> Result<(), cansocket::Error> {
    let socket = cansocket::Socket::new("vcan0")?;
    socket.set_recv_own_msgs(true)?;
    socket.set_protocol_errors(true)?;

    println!("Sending a frame.");
    let id = embedded_can::StandardId::new(0x123).unwrap();
    let frame = cansocket::Frame::new(id, &[4, 5, 6, 7]).unwrap();
    socket.send(&frame).await?;

    println!("Receiving a frame.");
    let frame = socket.recv().await;
    println!("Received: {:?}", frame);

    Ok(())
}
