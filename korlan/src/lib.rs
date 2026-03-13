#![doc = include_str!("../README.md")]

use nusb::{
    DeviceInfo, Interface, MaybeFuture,
    transfer::{Buffer, Bulk, Completion, In, Out, TransferError},
};
use std::time::Duration;
use tokio::sync::Mutex;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

const VENDOR_ID: u16 = 0x0483;
const PRODUCT_ID: u16 = 0x1234;

const EP_DATA_RX: u8 = 0x81; // endpoint 1 IN
const EP_DATA_TX: u8 = 0x02; // endpoint 2 OUT
const EP_CMD_RX: u8 = 0x83; // endpoint 3 IN
const EP_CMD_TX: u8 = 0x04; // endpoint 4 OUT

pub const ABP_CLOCK: u32 = 32_000_000;

const FLAG_SILENT: u32 = 0x01;
const FLAG_LOOPBACK: u32 = 0x02;
const FLAG_NO_AUTO_RETRANSMIT: u32 = 0x04;
const FLAG_STATUS_FRAME: u32 = 0x08;

const CMD_RESET: u8 = 1;
const CMD_OPEN: u8 = 2;
const CMD_CLOSE: u8 = 3;
const CMD_GET_SOFTW_HARDW_VER: u8 = 12;
const BAUD_MANUAL: u8 = 0x09;
const CMD_START: u8 = 0x11;
const CMD_END: u8 = 0x22;
const CMD_TIMEOUT: Duration = Duration::from_millis(1000);

const DATA_START: u8 = 0x55;
const DATA_END: u8 = 0xAA;
const TYPE_CAN_FRAME: u8 = 0;
const TYPE_ERROR_FRAME: u8 = 3;
const FLAG_EXTID: u8 = 0x01;
const FLAG_RTR: u8 = 0x02;
const FLAG_ERR: u8 = 0x04;

const STATUS_OK: u8 = 0x00;
const STATUS_OVERRUN: u8 = 0x01;
const STATUS_BUSLIGHT: u8 = 0x02;
const STATUS_BUSHEAVY: u8 = 0x03;
const STATUS_BUSOFF: u8 = 0x04;
const STATUS_STUFF: u8 = 0x20;
const STATUS_FORM: u8 = 0x21;
const STATUS_ACK: u8 = 0x23;
const STATUS_BIT0: u8 = 0x24;
const STATUS_BIT1: u8 = 0x25;
const STATUS_CRC: u8 = 0x27;

/// Command message: 16 bytes, big-endian multi-byte fields.
#[repr(C, packed)]
#[derive(Clone, Copy, Default, Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct CmdMsg {
    begin: u8,
    channel: u8,    // always 0
    command: u8,    // command to excute
    opt1: u8,       // optional parameter or return value
    opt2: u8,       // optional parameter
    data: [u8; 10], // optional parameter and data
    end: u8,
}

const CMD_MSG_LEN: usize = size_of::<CmdMsg>();

impl CmdMsg {
    fn new(command: u8) -> Self {
        Self {
            begin: CMD_START,
            command,
            end: CMD_END,
            ..Default::default()
        }
    }

    fn to_bytes(self) -> [u8; CMD_MSG_LEN] {
        let mut arr = [0u8; CMD_MSG_LEN];
        arr.copy_from_slice(self.as_bytes());
        arr
    }

    fn from_bytes(buf: &[u8]) -> Option<Self> {
        Self::read_from_prefix(buf).ok().map(|(msg, _)| msg)
    }
}

/// TX data message: 15 bytes.
#[repr(C, packed)]
#[derive(Clone, Copy, Default, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct TxMsg {
    begin: u8,
    flags: u8,
    id: [u8; 4], // big-endian
    dlc: u8,
    data: [u8; 8],
    end: u8,
}

const TX_MSG_LEN: usize = size_of::<TxMsg>();

impl TxMsg {
    fn to_bytes(self) -> [u8; TX_MSG_LEN] {
        let mut arr = [0u8; TX_MSG_LEN];
        arr.copy_from_slice(self.as_bytes());
        arr
    }
}

/// RX data message: 20 bytes.
#[repr(C, packed)]
#[derive(Clone, Copy, Default, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct RxMsg {
    begin: u8,
    msg_type: u8,
    flags: u8,
    id: [u8; 4], // big-endian
    dlc: u8,
    data: [u8; 8],
    timestamp: [u8; 4], // big-endian, ignored
    end: u8,
}

const RX_MSG_LEN: usize = size_of::<RxMsg>();

/// A CAN frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// CAN identifier (11-bit standard or 29-bit extended).
    pub id: u32,
    /// Data bytes.
    pub data: [u8; 8],
    /// Data length code (0-8).
    pub dlc: u8,
    /// Extended (29-bit) identifier flag.
    pub ext: bool,
    /// Remote transmission request flag.
    pub rtr: bool,
}

/// A bus error/status event decoded from a device error frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusError {
    Ok,
    Overrun,
    BusLight { tx_err: u8, rx_err: u8 },
    BusHeavy { tx_err: u8, rx_err: u8 },
    BusOff,
    Stuff,
    Form,
    Ack,
    Bit0,
    Bit1,
    Crc,
    Unknown(u8),
}

/// Items that may be received from the device.
#[derive(Debug, Clone)]
pub enum CanMessage {
    Frame(Frame),
    Error(BusError),
}

/// Mode flags for [`Device::open_with`].
#[derive(Debug, Clone, Default)]
pub struct ChannelOptions {
    pub loopback: bool,
    pub listen_only: bool,
    pub no_auto_retransmit: bool,
}

/// Firmware + hardware version returned by the device.
#[derive(Debug, Clone)]
pub struct Version {
    pub fw_major: u8,
    pub fw_minor: u8,
    pub hw_major: u8,
    pub hw_minor: u8,
}

/// Bittiming parameters sent to the device during channel open.
#[derive(Debug, Clone)]
pub struct BitTiming {
    /// prop_seg + phase_seg1 (1-16).
    pub tseg1: u8,
    /// phase_seg2 (1-8).
    pub tseg2: u8,
    /// Synchronisation jump width (1-4).
    pub sjw: u8,
    /// Baud-rate prescaler (1-1024).
    pub brp: u16,
}

impl BitTiming {
    /// Calculate bittiming parameters for the given bitrate against the device's
    /// 32 MHz clock, honouring the hardware constraints:
    /// - tseg1 ∈ 1..=16
    /// - tseg2 ∈ 1..=8
    /// - sjw ∈ 1..=4
    /// - brp ∈ 1..=1024
    ///
    /// Minimises the absolute bitrate error, preferring higher time-quantum counts
    /// (better sample-point accuracy) among equal-error candidates.
    pub fn from_bitrate(bitrate: u32) -> BitTiming {
        let mut best = BitTiming {
            tseg1: 13,
            tseg2: 2,
            sjw: 1,
            brp: 4,
        };
        let mut best_err = u64::MAX;

        for brp in 1u32..=1024 {
            for tseg1 in 1u32..=16 {
                for tseg2 in 1u32..=8 {
                    let tq = 1 + tseg1 + tseg2;
                    let actual = ABP_CLOCK / (brp * tq);
                    let err = (actual as i64 - bitrate as i64).unsigned_abs();
                    if err < best_err {
                        best_err = err;
                        best = BitTiming {
                            tseg1: tseg1 as u8,
                            tseg2: tseg2 as u8,
                            sjw: (tseg2.min(4)) as u8,
                            brp: brp as u16,
                        };
                    }
                }
            }
        }

        best
    }
}

/// Korlan errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("USB error: {0}")]
    Usb(#[from] nusb::Error),
    #[error("USB transfer error: {0}")]
    Transfer(#[from] TransferError),
    #[error("Command failed or malformed response from device")]
    Protocol,
    #[error("Not a USB2CAN converter (product string mismatch)")]
    NotRecognized,
}

/// List all connected Korlan USB2CAN devices.
pub fn list_devices() -> Result<impl Iterator<Item = DeviceInfo>, Error> {
    let iter = nusb::list_devices()
        .wait()
        .map_err(Error::Usb)?
        .filter(|d| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID);
    Ok(iter)
}

/// An open Korlan USB2CAN device.
// Endpoint handles are wrapped in `Mutex` so the device can be shared across
// async tasks (e.g. split send/recv loops).
pub struct Device {
    cmd_tx: Mutex<nusb::Endpoint<Bulk, Out>>,
    cmd_rx: Mutex<nusb::Endpoint<Bulk, In>>,
    data_tx: Mutex<nusb::Endpoint<Bulk, Out>>,
    data_rx: Mutex<nusb::Endpoint<Bulk, In>>,
    /// Partial receive buffer - accumulates bytes across USB transfers.
    rx_buf: Mutex<Vec<u8>>,
}

impl Device {
    /// Open the device identified by `info`.
    ///
    /// Verifies the product string, claims interface 0, and sends a reset
    /// command to confirm communication is working.
    pub async fn open(info: DeviceInfo) -> Result<Self, Error> {
        if info.product_string() != Some("USB2CAN converter") {
            return Err(Error::NotRecognized);
        }

        let device = info.open().await.map_err(Error::Usb)?;
        // On macOS the device starts unconfigured (value 0); set_configuration
        // creates the IOUSBInterface objects that claim_interface needs.
        // On Linux this is a no-op when config 1 is already active.
        device.set_configuration(1).await.map_err(Error::Usb)?;
        let iface: Interface = device.claim_interface(0).await.map_err(Error::Usb)?;

        let this = Self {
            cmd_tx: Mutex::new(iface.endpoint::<Bulk, Out>(EP_CMD_TX).map_err(Error::Usb)?),
            cmd_rx: Mutex::new(iface.endpoint::<Bulk, In>(EP_CMD_RX).map_err(Error::Usb)?),
            data_tx: Mutex::new(
                iface
                    .endpoint::<Bulk, Out>(EP_DATA_TX)
                    .map_err(Error::Usb)?,
            ),
            data_rx: Mutex::new(iface.endpoint::<Bulk, In>(EP_DATA_RX).map_err(Error::Usb)?),
            rx_buf: Mutex::new(Vec::new()),
        };

        this.reset().await?;
        Ok(this)
    }

    async fn send_cmd(&self, out: CmdMsg) -> Result<CmdMsg, Error> {
        let out_buf: Buffer = out.to_bytes().to_vec().into();

        let completion: Completion = {
            let mut ep = self.cmd_tx.lock().await;
            tokio::task::block_in_place(|| ep.transfer_blocking(out_buf, CMD_TIMEOUT))
        };
        completion.into_result().map_err(Error::Transfer)?;

        // IN transfers must be a multiple of the endpoint's max packet size
        // (64 bytes). The device sends exactly 16 bytes which arrives as one
        // short packet, so actual_len will be 16 even though we requested 64.
        let in_buf = Buffer::new(64);
        let completion: Completion = {
            let mut ep = self.cmd_rx.lock().await;
            tokio::task::block_in_place(|| ep.transfer_blocking(in_buf, CMD_TIMEOUT))
        };
        let resp_buf = completion.into_result().map_err(Error::Transfer)?;

        let resp = CmdMsg::from_bytes(&resp_buf).ok_or(Error::Protocol)?;
        if resp.begin != CMD_START || resp.end != CMD_END || resp.opt1 != 0 {
            return Err(Error::Protocol);
        }
        Ok(resp)
    }

    /// Drain any stale command responses left in the RX pipe from a previous
    /// session. We attempt up to 8 reads with a short timeout and stop as
    /// soon as one times out (nothing left) or we see a reset-ack.
    async fn drain_cmd_rx(&self) {
        let mut ep = self.cmd_rx.lock().await;
        for _ in 0..8 {
            let buf = Buffer::new(64);
            let c = tokio::task::block_in_place(|| {
                ep.transfer_blocking(buf, Duration::from_millis(50))
            });
            match c.into_result() {
                Err(_) => break, // timeout or error -> pipe is empty
                Ok(b) => {
                    // If this is a reset-ack (command byte 1, opt1=0) we're done
                    if b.len() >= CMD_MSG_LEN {
                        if let Some(msg) = CmdMsg::from_bytes(&b) {
                            if msg.command == CMD_RESET && msg.opt1 == 0 {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Send a device reset command and drain any stale responses first.
    pub async fn reset(&self) -> Result<(), Error> {
        // Send reset, then drain the pipe: any queued responses (from a prior
        // session) plus the reset-ack itself will be consumed by drain_cmd_rx.
        let out_buf: Buffer = CmdMsg::new(CMD_RESET).to_bytes().to_vec().into();
        {
            let mut ep = self.cmd_tx.lock().await;
            let c = tokio::task::block_in_place(|| ep.transfer_blocking(out_buf, CMD_TIMEOUT));
            c.into_result().map_err(Error::Transfer)?;
        }
        self.drain_cmd_rx().await;
        Ok(())
    }

    /// Query firmware and hardware version.
    pub async fn version(&self) -> Result<Version, Error> {
        let resp = self.send_cmd(CmdMsg::new(CMD_GET_SOFTW_HARDW_VER)).await?;
        Ok(Version {
            fw_major: resp.data[0],
            fw_minor: resp.data[1],
            hw_major: resp.data[2],
            hw_minor: resp.data[3],
        })
    }

    /// Open the CAN channel with the given bittiming and mode options.
    ///
    /// Must be called before [`send`][Self::send] or [`recv`][Self::recv].
    pub async fn open_with(&self, bt: BitTiming, opts: ChannelOptions) -> Result<(), Error> {
        let mut msg = CmdMsg::new(CMD_OPEN);
        msg.opt1 = BAUD_MANUAL;
        // data[0]: tseg1 (prop_seg + phase_seg1)
        // data[1]: tseg2 (phase_seg2)
        // data[2]: sjw
        // data[3..5]: BRP big-endian u16
        // data[5..9]: flags big-endian u32
        msg.data[0] = bt.tseg1;
        msg.data[1] = bt.tseg2;
        msg.data[2] = bt.sjw;
        let brp_be = bt.brp.to_be_bytes();
        msg.data[3] = brp_be[0];
        msg.data[4] = brp_be[1];
        let mut flags: u32 = FLAG_STATUS_FRAME;
        if opts.loopback {
            flags |= FLAG_LOOPBACK;
        }
        if opts.listen_only {
            flags |= FLAG_SILENT;
        }
        if opts.no_auto_retransmit {
            flags |= FLAG_NO_AUTO_RETRANSMIT;
        }
        let fb = flags.to_be_bytes();
        msg.data[5] = fb[0];
        msg.data[6] = fb[1];
        msg.data[7] = fb[2];
        msg.data[8] = fb[3];
        self.send_cmd(msg).await?;
        Ok(())
    }

    /// Convenience: open channel at a standard bitrate with default options.
    pub async fn set_bitrate(&self, bitrate: u32) -> Result<(), Error> {
        self.open_with(BitTiming::from_bitrate(bitrate), ChannelOptions::default())
            .await
    }

    /// Close the CAN channel.
    pub async fn close_channel(&self) -> Result<(), Error> {
        self.send_cmd(CmdMsg::new(CMD_CLOSE)).await?;
        Ok(())
    }

    /// Transmit a CAN frame.
    pub async fn send(&self, frame: Frame) -> Result<(), Error> {
        let mut msg = TxMsg {
            begin: DATA_START,
            end: DATA_END,
            ..Default::default()
        };
        if frame.rtr {
            msg.flags |= FLAG_RTR;
        }
        if frame.ext {
            msg.flags |= FLAG_EXTID;
        }
        msg.id = (frame.id & 0x1FFF_FFFF).to_be_bytes();
        msg.dlc = frame.dlc.min(8);
        msg.data = frame.data;

        let buf: Buffer = msg.to_bytes().to_vec().into();
        let completion: Completion = {
            let mut ep = self.data_tx.lock().await;
            tokio::task::block_in_place(|| ep.transfer_blocking(buf, Duration::from_secs(1)))
        };
        completion.into_result().map_err(Error::Transfer)?;
        Ok(())
    }

    /// Receive the next [`CanMessage`], blocking until one arrives.
    ///
    /// Internally accumulates bytes across USB bulk transfers and parses
    /// complete records, discarding unrecognized frames.
    pub async fn recv(&self) -> Result<CanMessage, Error> {
        loop {
            // Try to parse from the existing buffer first.
            {
                let mut buf = self.rx_buf.lock().await;
                if buf.len() >= RX_MSG_LEN {
                    let chunk: Vec<u8> = buf.drain(..RX_MSG_LEN).collect();
                    if let Some(msg) = parse_rx_msg(&chunk) {
                        return Ok(msg);
                    }
                    // Unrecognized frame type - keep consuming.
                    continue;
                }
            }

            // Fetch more data from the device.
            // Requested length must be a multiple of max_packet_size (64).
            let rx = Buffer::new(512); // 512 = 8 × 64
            let completion: Completion = {
                let mut ep = self.data_rx.lock().await;
                tokio::task::block_in_place(|| ep.transfer_blocking(rx, Duration::from_secs(5)))
            };
            let filled = completion.into_result().map_err(Error::Transfer)?;
            self.rx_buf.lock().await.extend_from_slice(&filled);
        }
    }
}

fn parse_rx_msg(buf: &[u8]) -> Option<CanMessage> {
    if buf.len() < RX_MSG_LEN {
        return None;
    }
    let Ok((msg, _)) = RxMsg::read_from_prefix(buf) else {
        return None;
    };

    if msg.msg_type == TYPE_ERROR_FRAME && msg.flags == FLAG_ERR {
        let state = msg.data[0];
        let rx_err = msg.data[1] & 0x7F;
        let tx_err = msg.data[2];
        let err = match state {
            STATUS_OK => BusError::Ok,
            STATUS_OVERRUN => BusError::Overrun,
            STATUS_BUSLIGHT => BusError::BusLight { tx_err, rx_err },
            STATUS_BUSHEAVY => BusError::BusHeavy { tx_err, rx_err },
            STATUS_BUSOFF => BusError::BusOff,
            STATUS_STUFF => BusError::Stuff,
            STATUS_FORM => BusError::Form,
            STATUS_ACK => BusError::Ack,
            STATUS_BIT0 => BusError::Bit0,
            STATUS_BIT1 => BusError::Bit1,
            STATUS_CRC => BusError::Crc,
            other => BusError::Unknown(other),
        };
        return Some(CanMessage::Error(err));
    }

    if msg.msg_type == TYPE_CAN_FRAME {
        let id = u32::from_be_bytes(msg.id);
        let ext = (msg.flags & FLAG_EXTID) != 0;
        let rtr = (msg.flags & FLAG_RTR) != 0;
        return Some(CanMessage::Frame(Frame {
            id,
            data: msg.data,
            dlc: msg.dlc & 0xF,
            ext,
            rtr,
        }));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msg_sizes() {
        // CmdMsg: begin+channel+command+opt1+opt2+data[10]+end = 16 bytes
        assert_eq!(CMD_MSG_LEN, 16);
        // TxMsg: begin+flags+id[4]+dlc+data[8]+end = 16 bytes
        assert_eq!(TX_MSG_LEN, 16);
        // RxMsg: begin+type+flags+id[4]+dlc+data[8]+timestamp[4]+end = 21 bytes
        assert_eq!(RX_MSG_LEN, 21);
    }

    #[test]
    fn cmd_msg_roundtrip() {
        let msg = CmdMsg::new(CMD_GET_SOFTW_HARDW_VER);
        let bytes = msg.to_bytes();
        assert_eq!(bytes[0], CMD_START);
        assert_eq!(bytes[CMD_MSG_LEN - 1], CMD_END);
        assert_eq!(bytes.len(), CMD_MSG_LEN);

        let back = CmdMsg::from_bytes(&bytes).unwrap();
        assert_eq!(back.begin, CMD_START);
        assert_eq!(back.command, CMD_GET_SOFTW_HARDW_VER);
        assert_eq!(back.end, CMD_END);
    }

    fn bitrate_error(bitrate: u32) -> i32 {
        let bt = BitTiming::from_bitrate(bitrate);
        let tq = 1 + bt.tseg1 as u32 + bt.tseg2 as u32;
        let actual = ABP_CLOCK / (bt.brp as u32 * tq);
        actual as i32 - bitrate as i32
    }

    #[test]
    fn bittiming_500k() {
        assert!(bitrate_error(500_000).abs() < 5_000);
    }
    #[test]
    fn bittiming_250k() {
        assert!(bitrate_error(250_000).abs() < 2_500);
    }
    #[test]
    fn bittiming_125k() {
        assert!(bitrate_error(125_000).abs() < 1_250);
    }
    #[test]
    fn bittiming_1m() {
        assert!(bitrate_error(1_000_000).abs() < 10_000);
    }

    fn make_rx_buf() -> [u8; RX_MSG_LEN] {
        [0u8; RX_MSG_LEN]
    }

    #[test]
    fn parse_std_can_frame() {
        let mut b = make_rx_buf();
        b[0] = DATA_START;
        b[1] = TYPE_CAN_FRAME;
        b[2] = 0; // standard id, no RTR
        // id = 0x00000123 BE
        b[3] = 0x00;
        b[4] = 0x00;
        b[5] = 0x01;
        b[6] = 0x23;
        b[7] = 4; // dlc
        b[8..12].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        b[RX_MSG_LEN - 1] = DATA_END;

        let msg = parse_rx_msg(&b).unwrap();
        match msg {
            CanMessage::Frame(f) => {
                assert_eq!(f.id, 0x123);
                assert!(!f.ext);
                assert!(!f.rtr);
                assert_eq!(f.dlc, 4);
                assert_eq!(&f.data[..4], &[0xDE, 0xAD, 0xBE, 0xEF]);
            }
            _ => panic!("expected Frame, got {msg:?}"),
        }
    }

    #[test]
    fn parse_ext_can_frame() {
        let mut b = make_rx_buf();
        b[0] = DATA_START;
        b[1] = TYPE_CAN_FRAME;
        b[2] = FLAG_EXTID;
        let id: [u8; 4] = 0x1234_5678u32.to_be_bytes();
        b[3..7].copy_from_slice(&id);
        b[7] = 8;
        b[8..16].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        b[RX_MSG_LEN - 1] = DATA_END;

        let msg = parse_rx_msg(&b).unwrap();
        match msg {
            CanMessage::Frame(f) => {
                assert_eq!(f.id, 0x1234_5678);
                assert!(f.ext);
                assert_eq!(f.dlc, 8);
            }
            _ => panic!("expected Frame"),
        }
    }

    #[test]
    fn parse_rtr_frame() {
        let mut b = make_rx_buf();
        b[0] = DATA_START;
        b[1] = TYPE_CAN_FRAME;
        b[2] = FLAG_RTR;
        b[3..7].copy_from_slice(&0x42u32.to_be_bytes());
        b[7] = 0;
        b[RX_MSG_LEN - 1] = DATA_END;

        match parse_rx_msg(&b).unwrap() {
            CanMessage::Frame(f) => assert!(f.rtr),
            _ => panic!("expected Frame"),
        }
    }

    #[test]
    fn parse_error_busoff() {
        let mut b = make_rx_buf();
        b[0] = DATA_START;
        b[1] = TYPE_ERROR_FRAME;
        b[2] = FLAG_ERR;
        // data[0] is at offset 8: begin+type+flags+id[4]+dlc = 8 bytes before data
        b[8] = STATUS_BUSOFF;
        b[RX_MSG_LEN - 1] = DATA_END;

        assert!(matches!(
            parse_rx_msg(&b),
            Some(CanMessage::Error(BusError::BusOff))
        ));
    }

    #[test]
    fn parse_error_buslight() {
        let mut b = make_rx_buf();
        b[0] = DATA_START;
        b[1] = TYPE_ERROR_FRAME;
        b[2] = FLAG_ERR;
        // data[0..2] are at offsets 8..10
        b[8] = STATUS_BUSLIGHT;
        b[9] = 0x10; // rx_err (bit 7 = RP flag, cleared here)
        b[10] = 0x20; // tx_err
        b[RX_MSG_LEN - 1] = DATA_END;

        assert!(matches!(
            parse_rx_msg(&b),
            Some(CanMessage::Error(BusError::BusLight {
                tx_err: 0x20,
                rx_err: 0x10
            }))
        ));
    }

    #[test]
    fn parse_error_variants() {
        let cases: &[(u8, BusError)] = &[
            (STATUS_OK, BusError::Ok),
            (STATUS_OVERRUN, BusError::Overrun),
            (
                STATUS_BUSHEAVY,
                BusError::BusHeavy {
                    tx_err: 0,
                    rx_err: 0,
                },
            ),
            (STATUS_STUFF, BusError::Stuff),
            (STATUS_FORM, BusError::Form),
            (STATUS_ACK, BusError::Ack),
            (STATUS_BIT0, BusError::Bit0),
            (STATUS_BIT1, BusError::Bit1),
            (STATUS_CRC, BusError::Crc),
            (0xFF, BusError::Unknown(0xFF)),
        ];
        for (status, expected) in cases {
            let mut b = make_rx_buf();
            b[1] = TYPE_ERROR_FRAME;
            b[2] = FLAG_ERR;
            // data[0] is at offset 8
            b[8] = *status;
            let got = parse_rx_msg(&b).unwrap();
            match (got, expected) {
                (CanMessage::Error(BusError::Ok), BusError::Ok) => {}
                (CanMessage::Error(BusError::Overrun), BusError::Overrun) => {}
                (CanMessage::Error(BusError::BusHeavy { .. }), BusError::BusHeavy { .. }) => {}
                (CanMessage::Error(BusError::Stuff), BusError::Stuff) => {}
                (CanMessage::Error(BusError::Form), BusError::Form) => {}
                (CanMessage::Error(BusError::Ack), BusError::Ack) => {}
                (CanMessage::Error(BusError::Bit0), BusError::Bit0) => {}
                (CanMessage::Error(BusError::Bit1), BusError::Bit1) => {}
                (CanMessage::Error(BusError::Crc), BusError::Crc) => {}
                (CanMessage::Error(BusError::Unknown(v)), BusError::Unknown(e)) => {
                    assert_eq!(v, *e)
                }
                (got, exp) => panic!("status {status:#x}: got {got:?}, expected {exp:?}"),
            }
        }
    }

    #[test]
    fn parse_too_short_returns_none() {
        let b = [0u8; RX_MSG_LEN - 1];
        assert!(parse_rx_msg(&b).is_none());
    }
}

/// These tests require a physical device.
///
/// Run with:
/// ```text
/// cargo test -p korlan -- --ignored
/// ```
#[cfg(test)]
mod integration {
    use super::*;
    use std::time::Duration;

    /// Serialise all hardware tests: the device can only be opened exclusively
    /// by one handle at a time, so tests must not run concurrently.
    static HW_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

    fn first_device() -> Option<DeviceInfo> {
        list_devices().ok()?.next()
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires hardware"]
    async fn test_list() {
        let _guard = HW_LOCK.lock().await;
        let devices: Vec<_> = list_devices().expect("list_devices failed").collect();
        assert!(!devices.is_empty(), "no Korlan device found");
        for d in &devices {
            println!(
                "  {:04x}:{:04x}  {:?}",
                d.vendor_id(),
                d.product_id(),
                d.product_string()
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires hardware"]
    async fn test_open_and_version() {
        let _guard = HW_LOCK.lock().await;
        let info = first_device().expect("no device");
        let dev = Device::open(info).await.expect("open failed");
        let ver = dev.version().await.expect("version failed");
        println!(
            "Firmware {}.{}, Hardware {}.{}",
            ver.fw_major, ver.fw_minor, ver.hw_major, ver.hw_minor
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires hardware"]
    async fn test_set_bitrate() {
        let _guard = HW_LOCK.lock().await;
        let info = first_device().expect("no device");
        let dev = Device::open(info).await.expect("open failed");
        dev.set_bitrate(500_000).await.expect("set_bitrate failed");
        dev.close_channel().await.expect("close failed");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires hardware"]
    async fn test_loopback_send_recv() {
        let _guard = HW_LOCK.lock().await;
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
            id: 0x42,
            data: [1, 2, 3, 4, 0, 0, 0, 0],
            dlc: 4,
            ext: false,
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
    #[ignore = "requires hardware"]
    async fn test_extended_frame_loopback() {
        let _guard = HW_LOCK.lock().await;
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
            id: 0x1234_5678,
            data: [0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0x00, 0x01],
            dlc: 8,
            ext: true,
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
                assert!(f.ext);
            }
            CanMessage::Error(e) => panic!("got error: {e:?}"),
        }

        dev.close_channel().await.expect("close failed");
    }
}
