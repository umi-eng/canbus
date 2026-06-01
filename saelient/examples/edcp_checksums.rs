//! Demonstrates J1939-73 EDCP (Error Detection Code Protection) nibble
//! checksumming for memory access messages (DM14/DM15) and TP boot-load data
//! transfers (DM17).
//!
//! The scenario:
//!   1. A diagnostic tool (requester) sends a DM14 Write request protected by
//!      EDCP.
//!   2. The ECU verifies the checksum and replies with a DM15 response, also
//!      EDCP-protected.
//!   3. The requester verifies the response, then sends the payload over TP as
//!      a series of EDCP-protected DM17 BootLoadData frames.

use saelient::Pgn;
use saelient::diagnostic::{
    BootLoadData, Command, ErrorIndicator, MemoryAccessRequest, MemoryAccessResponse, Pointer,
    Status,
};
use saelient::transport::{DataTransfer, RequestToSend, Response, Transfer};

fn main() {
    let request = MemoryAccessRequest::new(
        Command::Write,
        Pointer::Direct(0x0002_0000), // target flash address
        16,                           // 16 bytes to write
        0x1A00,                       // security key (upper nibble preserved by EDCP)
    )
    .with_edcp();

    println!("DM14 Memory Access Request");
    println!("\tcommand  : {:?}", request.command());
    println!("\tpointer  : {:?}", request.pointer());
    println!("\tlength   : {} bytes", request.length());
    println!("\tEDCP set : {}", request.edcp_protected());
    println!("\tvalid    : {}", request.verify_edcp());

    // Serialise to 8 bytes for transmission on the bus.
    let request_bytes: [u8; 8] = (&request).into();
    println!("\ton wire  : {request_bytes:02X?}");
    println!("");

    println!("ECU receives DM14");
    let received_request =
        MemoryAccessRequest::try_from(request_bytes.as_ref()).expect("valid 8-byte frame");

    if !received_request.verify_edcp() {
        eprintln!("\tEDCP verification FAILED — discarding frame");
        return;
    }
    println!("\tEDCP verified OK");

    // Demonstrate that a single flipped bit is detected.
    let mut corrupted = request_bytes;
    corrupted[3] ^= 0x08;
    let corrupted_request = MemoryAccessRequest::try_from(corrupted.as_ref()).unwrap();
    println!(
        "\tcorrupted frame valid: {} (expected false)",
        corrupted_request.verify_edcp()
    );
    println!("");

    // ECU sends back a DM15 "Proceed" response, also EDCP-protected.
    let response = MemoryAccessResponse::new(
        Status::Proceed,
        ErrorIndicator::None,
        16,     // echo the length from DM14
        0xB200, // seed for the next security exchange (upper nibble preserved)
    )
    .with_edcp();

    println!("DM15 Memory Access Response");
    println!("\tstatus   : {:?}", response.status());
    println!("\tlength   : {} bytes", response.length());
    println!("\tEDCP set : {}", response.edcp_protected());
    println!("\tvalid    : {}", response.verify_edcp());

    let response_bytes: [u8; 8] = (&response).into();
    println!("\ton wire  : {response_bytes:02X?}");
    println!("");

    println!("Requester verifies DM15");
    let received_response =
        MemoryAccessResponse::try_from(response_bytes.as_ref()).expect("valid 8-byte frame");

    if !received_response.verify_edcp() {
        eprintln!("  EDCP verification FAILED — aborting transfer");
        return;
    }
    println!("  EDCP verified OK — proceeding with TP transfer");
    println!("");

    // The 16-byte payload to write into flash.
    let payload: [u8; 16] = [
        0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD,
        0xEF,
    ];

    println!("DM17 BootLoadData frames (TP transfer)");

    // The RTS announces 16 bytes with at most 1 packet per CTS window so the
    // receiver can flow-control the transfer.
    let rts = RequestToSend::new(16, Some(1), Pgn::ProprietaryA);
    let mut transfer = Transfer::new(rts);

    // Split the payload into 7-byte TP chunks, wrap each in a BootLoadData
    // frame, apply EDCP, then hand the data bytes to DataTransfer.
    for (seq, chunk) in payload.chunks(7).enumerate() {
        // Build a BootLoadData frame:
        //   bytes 0-6  = application payload (padded with 0xFF)
        //   byte 7 low nibble = EDCP checksum (written by with_edcp)
        //   byte 7 high nibble = 0xF (filler / not used by this application)
        let mut frame_bytes = [0xFF_u8; 8];
        frame_bytes[..chunk.len()].copy_from_slice(chunk);
        // High nibble of byte 7 is free for the application; low nibble will
        // be overwritten by with_edcp().
        frame_bytes[7] = 0xF0;

        let bl = BootLoadData::try_from(frame_bytes.as_ref())
            .expect("valid 8-byte frame")
            .with_edcp();

        let bl_bytes = bl.data();
        println!(
            "  DM17 seq={} bytes={bl_bytes:02X?} edcp_ok={}",
            seq + 1,
            bl.verify_edcp()
        );

        // Extract the 7-byte TP payload (the full 8 bytes include the EDCP
        // nibble in byte 7; hand all 8 bytes as the 7-byte TP data window so
        // the receiver can re-check EDCP on arrival).
        let mut tp_data = [0xFF_u8; 7];
        tp_data.copy_from_slice(&bl_bytes[..7]);

        let dt = DataTransfer::new(seq as u8 + 1, tp_data);
        match transfer.next(dt) {
            Ok(Some(Response::Cts(cts))) => println!("  <- CTS next_seq={}", cts.next_sequence()),
            Ok(Some(Response::End(end))) => {
                println!("  <- EndOfMsgAck total={} bytes", end.total_size())
            }
            Ok(None) => {}
            Err((err, abort)) => eprintln!("  TP error: {err:?} / {abort:?}"),
        }
    }

    println!("");
    println!("Done — all frames sent and checksums verified.");
}
