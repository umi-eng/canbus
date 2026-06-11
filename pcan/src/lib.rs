#![doc = include_str!("../README.md")]

mod frame;

pub use frame::FdFrame;
pub use frame::Frame;

use frame::fd_dlc_to_len;
use frame::fd_len_to_dlc;
use futures::lock::Mutex;
use nusb::Interface;
use nusb::transfer::Buffer;
use nusb::transfer::Bulk;
use nusb::transfer::Completion;
use nusb::transfer::ControlIn;
use nusb::transfer::ControlOut;
use nusb::transfer::ControlType;
use nusb::transfer::In;
use nusb::transfer::Out;
use nusb::transfer::Recipient;
use nusb::transfer::TransferError;
use std::time::Duration;
use zerocopy::FromBytes;
use zerocopy::IntoBytes;
use zerocopy::TryFromBytes;

/// PEAK-System vendor id.
pub const VENDOR_ID: u16 = 0x0c72;
/// PCAN-USB FD product id.
pub const PCAN_USBFD_PRODUCT_ID: u16 = 0x0012;

const EP_CMD_OUT_DEFAULT: u8 = 0x01;
const EP_CMD_IN_DEFAULT: u8 = 0x81;
const EP_DATA_OUT_DEFAULT: u8 = 0x02;
const EP_DATA_IN_DEFAULT: u8 = 0x82;

const REQ_INFO: u8 = 0;
const REQ_FCT: u8 = 2;
const INFO_FW: u16 = 1;
const FCT_DRVLD: u16 = 5;
const FCT_DRVLD_LEN: usize = 16;
const CTRL_TIMEOUT: Duration = Duration::from_millis(2000);

const FW_INFO_LEN: usize = 36;
const FW_INFO_TYPE_EXT: u16 = 2;

/// Firmware information.
#[derive(Debug, FromBytes)]
#[repr(C)]
pub struct FirmwareInfo {
    pub size_of: u16,
    pub kind: u16,
    pub hardware_kind: u8,
    pub bootloader_version: [u8; 3],
    pub hardware_version: u8,
    pub firmware_version: [u8; 3],
    pub device_id: [u32; 2],
    pub serial_number: u32,
    pub flags: u32,
}

impl FirmwareInfo {
    const _SIZE: () = assert!(size_of::<Self>() == 8);
}

/// Extended firmware info.
#[derive(Debug, FromBytes)]
#[repr(C)]
struct FirmwareInfoExt {
    pub cmd_out_ep: u8,
    pub cmd_in_ep: u8,
    pub data_out_ep: [u8; 2],
    pub data_in_ep: u8,
    _reserved: [u8; 3],
}

impl FirmwareInfoExt {
    const _SIZE: () = assert!(size_of::<Self>() == 8);
}

/// PUCAN opcodes
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoBytes)]
#[repr(u16)]
enum OpCode {
    // PUCAN
    ResetMode = 0x001,
    NormalMode = 0x002,
    ListenOnlyMode = 0x003,
    TimingSlow = 0x004,
    TimingFast = 0x005,
    FilterStandard = 0x008,
    ErrorCounterWrite = 0x00a,
    SetEnableOption = 0x00b,
    // UFD-specific command opcodes (4-bit extension area, >= 0x80)
    ClockSet = 0x080,
    LedSet = 0x086,
}

impl OpCode {
    fn as_le_bytes(&self) -> [u8; 2] {
        (*self as u16 & 0x3ff).to_le_bytes()
    }
}

/// Tx error counter write enable
const PUCAN_WRERRCNT_TE: u16 = 0x4000;
/// Rx error counter write enable
const PUCAN_WRERRCNT_RE: u16 = 0x8000;

const PUCAN_OPTION_ERROR: u16 = 0x0001;
const PCAN_UFD_FLTEXT_CALIBRATION: u16 = 0x8000;

#[derive(Debug, Default)]
pub enum LedMode {
    #[default]
    Device = 0x00,
    Fast = 0x01,
    Slow = 0x02,
    On = 0x03,
    Off = 0x04,
}

const PUCAN_MSG_CAN_RX: u16 = 0x0001;
const PUCAN_MSG_ERROR: u16 = 0x0002;
const PUCAN_MSG_STATUS: u16 = 0x0003;
const PUCAN_MSG_CAN_TX: u16 = 0x1000;
const PCAN_UFD_MSG_CALIBRATION: u16 = 0x0100;
const PCAN_UFD_MSG_OVERRUN: u16 = 0x0101;

const PUCAN_MSG_EXT_DATA_LEN: u16 = 0x10; // CAN-FD frame
const PUCAN_MSG_BITRATE_SWITCH: u16 = 0x20;
const PUCAN_MSG_ERROR_STATE_IND: u16 = 0x40;
const PUCAN_MSG_EXT_ID: u16 = 0x02;
const PUCAN_MSG_RTR: u16 = 0x01;

const PUCAN_BUS_PASSIVE: u8 = 0x20;
const PUCAN_BUS_WARNING: u8 = 0x40;
const PUCAN_BUS_BUSOFF: u8 = 0x80;

/// Items received from the device.
#[derive(Debug, Clone)]
pub enum CanMessage {
    Frame(Frame),
    FdFrame(FdFrame),
    Status(BusStatus),
}

/// CAN bus error/state event from the device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusStatus {
    pub bus_off: bool,
    pub bus_warning: bool,
    pub bus_passive: bool,
    pub tx_err: u8,
    pub rx_err: u8,
}

/// Mode flags for [`PCanFd::open_channel`].
#[derive(Debug, Clone, Default)]
pub struct ChannelOptions {
    /// Listen-only mode
    pub listen_only: bool,
    /// One-shot mode
    pub one_shot: bool,
}

/// Nominal (arbitration-phase) bittiming for the 80 MHz µCAN clock.
///
/// Hardware constraints:
/// - `brp` 1..=1024
/// - `tseg1` 1..=256
/// - `tseg2` 1..=128
/// - `sjw` 1..=128
#[derive(Debug, Clone)]
pub struct BitTiming {
    pub tseg1: u16,
    pub tseg2: u8,
    pub sjw: u8,
    pub brp: u16,
}

impl BitTiming {
    /// Compute bittiming from a desired nominal bitrate.
    pub fn from_bitrate(bitrate: u32) -> Self {
        let mut best = Self {
            tseg1: 13,
            tseg2: 2,
            sjw: 1,
            brp: 1,
        };
        let mut best_err = u64::MAX;
        for brp in 1u32..=1024 {
            for tseg1 in 1u32..=256 {
                for tseg2 in 1u32..=128 {
                    let tq = 1 + tseg1 + tseg2;
                    let actual = ClockMode::default().as_hz() / (brp * tq);
                    let err = (actual as i64 - bitrate as i64).unsigned_abs();
                    if err < best_err {
                        best_err = err;
                        best = Self {
                            tseg1: tseg1 as u16,
                            tseg2: tseg2 as u8,
                            sjw: tseg2.min(127) as u8 + 1,
                            brp: brp as u16,
                        };
                    }
                }
            }
        }
        best
    }
}

/// Data-phase bittiming for CAN-FD (80 MHz µCAN clock).
///
/// Hardware constraints:
/// - `brp`  1..=1024
/// - `tseg1` 1..=32
/// - `tseg2` 1..=16
/// - `sjw` 1..=16
#[derive(Debug, Clone)]
pub struct DataBitTiming {
    pub tseg1: u8,
    pub tseg2: u8,
    pub sjw: u8,
    pub brp: u16,
}

impl DataBitTiming {
    /// Compute data bittiming from a desired data-phase bitrate.
    pub fn from_bitrate(bitrate: u32) -> Self {
        let mut best = Self {
            tseg1: 6,
            tseg2: 3,
            sjw: 1,
            brp: 1,
        };
        let mut best_err = u64::MAX;
        for brp in 1u32..=1024 {
            for tseg1 in 1u32..=32 {
                for tseg2 in 1u32..=16 {
                    let tq = 1 + tseg1 + tseg2;
                    let actual = ClockMode::default().as_hz() / (brp * tq);
                    let err = (actual as i64 - bitrate as i64).unsigned_abs();
                    if err < best_err {
                        best_err = err;
                        best = Self {
                            tseg1: tseg1 as u8,
                            tseg2: tseg2 as u8,
                            sjw: tseg2.min(15) as u8 + 1,
                            brp: brp as u16,
                        };
                    }
                }
            }
        }
        best
    }
}

/// Driver error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("USB error: {0}")]
    Usb(#[from] nusb::Error),
    #[error("USB transfer error: {0}")]
    Transfer(#[from] TransferError),
    #[error("Protocol error or malformed response")]
    Protocol,
}

/// List all connected PCAN-USB FD devices (VID 0x0c72, PID 0x0012).
pub async fn list_devices() -> Result<impl Iterator<Item = nusb::DeviceInfo>, Error> {
    let iter = nusb::list_devices()
        .await
        .map_err(Error::Usb)?
        .filter(|d| d.vendor_id() == VENDOR_ID && d.product_id() == PCAN_USBFD_PRODUCT_ID);
    Ok(iter)
}

/// An open PCAN-USB FD device.
pub struct PCanFd {
    /// Keep the device handle for control transfers.
    _usb_device: nusb::Device,
    cmd_out: Mutex<nusb::Endpoint<Bulk, Out>>,
    data_out: Mutex<nusb::Endpoint<Bulk, Out>>,
    data_in: Mutex<nusb::Endpoint<Bulk, In>>,
    /// Byte accumulator for partial RX packets.
    rx_buf: Mutex<Vec<u8>>,
    /// Firmware info read at open time.
    pub fw_info: FirmwareInfo,
}

impl PCanFd {
    /// Open a device from the list returned by [`list_devices`].
    pub async fn open(info: nusb::DeviceInfo) -> Result<Self, Error> {
        let usb_device = info.open().await.map_err(Error::Usb)?;
        let _ = usb_device.detach_kernel_driver(0);
        // Do NOT call device.reset() - on macOS it causes a re-enumeration
        // (IOKit IOUSBDevice::ResetDevice) that briefly disconnects the device,
        // making it unavailable for ~100 ms.  The PCAN-USB FD firmware does
        // not require a USB-level reset to reach a known state.
        usb_device.set_configuration(1).await.map_err(Error::Usb)?;

        let iface: Interface = usb_device.claim_interface(0).await.map_err(Error::Usb)?;

        let fw_raw = iface
            .control_in(
                ControlIn {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Other,
                    request: REQ_INFO,
                    value: INFO_FW,
                    index: 0,
                    length: FW_INFO_LEN as u16,
                },
                CTRL_TIMEOUT,
            )
            .await
            .map_err(Error::Transfer)?;

        let Ok((fw_info, fw_raw)) = FirmwareInfo::try_read_from_prefix(&fw_raw) else {
            return Err(Error::Protocol);
        };

        let (ep_cmd_out, _ep_cmd_in, ep_data_out, ep_data_in) = if fw_info.kind >= FW_INFO_TYPE_EXT
        {
            let Ok((fw_info_ext, _)) = FirmwareInfoExt::try_read_from_prefix(fw_raw) else {
                return Err(Error::Protocol);
            };

            (
                fw_info_ext.cmd_out_ep,
                fw_info_ext.cmd_in_ep,
                fw_info_ext.data_out_ep[0],
                fw_info_ext.data_in_ep,
            )
        } else {
            (
                EP_CMD_OUT_DEFAULT,
                EP_CMD_IN_DEFAULT,
                EP_DATA_OUT_DEFAULT,
                EP_DATA_IN_DEFAULT,
            )
        };

        let mut cmd_out = iface
            .endpoint::<Bulk, Out>(ep_cmd_out)
            .map_err(Error::Usb)?;
        let mut data_out = iface
            .endpoint::<Bulk, Out>(ep_data_out)
            .map_err(Error::Usb)?;
        let mut data_in = iface.endpoint::<Bulk, In>(ep_data_in).map_err(Error::Usb)?;

        let _ = cmd_out.clear_halt().await;
        let _ = data_out.clear_halt().await;
        let _ = data_in.clear_halt().await;

        // signal to the device the driver is loaded
        let mut drv_payload = [0u8; FCT_DRVLD_LEN];
        drv_payload[1] = 1; // loaded = true
        iface
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Other,
                    request: REQ_FCT,
                    value: FCT_DRVLD,
                    index: 0,
                    data: &drv_payload,
                },
                CTRL_TIMEOUT,
            )
            .await
            .map_err(Error::Transfer)?;

        let this = Self {
            _usb_device: usb_device,
            cmd_out: Mutex::new(cmd_out),
            data_out: Mutex::new(data_out),
            data_in: Mutex::new(data_in),
            rx_buf: Mutex::new(Vec::new()),
            fw_info,
        };

        this.send_cmd(&cmd_clock(ClockMode::default())).await?;
        this.send_cmd(&cmd_led(LedMode::default() as u8)).await?;

        Ok(this)
    }

    pub async fn set_led(&self, mode: LedMode) -> Result<(), Error> {
        self.send_cmd(&cmd_led(mode as u8)).await?;
        Ok(())
    }

    /// Open the CAN channel with the given nominal bittiming and options.
    ///
    /// For CAN-FD, also call [`set_data_bitrate`][Self::set_data_bitrate] before
    /// transmitting FD frames with BRS.
    pub async fn open_channel(&self, bt: BitTiming, opts: ChannelOptions) -> Result<(), Error> {
        self.set_filter_all_accept().await?;

        self.send_cmd(&cmd_option_set(
            OpCode::SetEnableOption,
            PUCAN_OPTION_ERROR,
            PCAN_UFD_FLTEXT_CALIBRATION,
        ))
        .await?;

        // reset error counters
        self.send_cmd(&cmd_error_counters_write(0, 0)).await?;

        // configure bit timing
        self.send_cmd(&cmd_timing_slow(&bt)).await?;

        // enable bus
        let op = if opts.listen_only {
            OpCode::ListenOnlyMode
        } else {
            OpCode::NormalMode
        };
        self.send_cmd(&cmd_opcode(op)).await?;

        Ok(())
    }

    /// Convenience: open channel at a standard nominal bitrate with default options.
    pub async fn set_bitrate(&self, bitrate: u32) -> Result<(), Error> {
        self.open_channel(BitTiming::from_bitrate(bitrate), ChannelOptions::default())
            .await
    }

    /// Set the CAN-FD data-phase bittiming.
    ///
    /// Call after [`open_channel`][Self::open_channel].
    pub async fn set_data_bitrate(&self, bitrate: u32) -> Result<(), Error> {
        self.send_cmd(&cmd_timing_fast(&DataBitTiming::from_bitrate(bitrate)))
            .await
    }

    /// Set CAN-FD data-phase bittiming from explicit parameters.
    pub async fn set_data_bittiming(&self, dbt: DataBitTiming) -> Result<(), Error> {
        self.send_cmd(&cmd_timing_fast(&dbt)).await
    }

    /// Close the CAN channel (put the controller back into reset mode).
    pub async fn close_channel(&self) -> Result<(), Error> {
        self.send_cmd(&cmd_opcode(OpCode::ResetMode)).await
    }

    /// Transmit a classical CAN frame.
    pub async fn send(&self, frame: Frame) -> Result<(), Error> {
        let (raw_id, ext) = match frame.id {
            embedded_can::Id::Standard(s) => (s.as_raw() as u32, false),
            embedded_can::Id::Extended(e) => (e.as_raw(), true),
        };

        let mut flags: u16 = 0;
        if ext {
            flags |= PUCAN_MSG_EXT_ID;
        }
        if frame.rtr {
            flags |= PUCAN_MSG_RTR;
        }

        let dlc = frame.dlc & 0xf;
        let data_len = if frame.rtr { 0 } else { frame.dlc as usize };

        self.send_tx_msg(raw_id, flags, dlc, &frame.data[..data_len])
            .await
    }

    /// Transmit a CAN-FD frame.
    pub async fn send_fd(&self, frame: FdFrame) -> Result<(), Error> {
        let (raw_id, ext) = match frame.id {
            embedded_can::Id::Standard(s) => (s.as_raw() as u32, false),
            embedded_can::Id::Extended(e) => (e.as_raw(), true),
        };

        let mut flags: u16 = PUCAN_MSG_EXT_DATA_LEN;
        if ext {
            flags |= PUCAN_MSG_EXT_ID;
        }
        if frame.brs {
            flags |= PUCAN_MSG_BITRATE_SWITCH;
        }
        if frame.esi {
            flags |= PUCAN_MSG_ERROR_STATE_IND;
        }

        let len = frame.len as usize;
        let dlc = fd_len_to_dlc(len);

        self.send_tx_msg(raw_id, flags, dlc, &frame.data[..len])
            .await
    }

    /// Receive the next [`CanMessage`], blocking until one arrives.
    ///
    /// Classical CAN frames, CAN-FD frames and bus-status events are returned.
    /// Calibration timestamps and other internal records are consumed silently.
    pub async fn recv(&self) -> Result<CanMessage, Error> {
        loop {
            if let Some(msg) = self.try_decode_one().await {
                return Ok(msg);
            }

            const BUF_SIZE_RX: usize = 2048;
            let rx = Buffer::new(BUF_SIZE_RX);
            let completion: Completion = {
                let mut ep = self.data_in.lock().await;
                ep.submit(rx);
                ep.next_complete().await
            };
            let filled = completion.into_result().map_err(Error::Transfer)?;
            if !filled.is_empty() {
                self.rx_buf.lock().await.extend_from_slice(&filled);
            }
        }
    }

    /// Send a single pucan command (8 bytes + 8-byte EOC) over the command endpoint.
    async fn send_cmd(&self, cmd: &[u8; 8]) -> Result<(), Error> {
        let mut buf = [0u8; 16]; // 8 cmd + 8 EOC
        buf[..8].copy_from_slice(cmd);
        buf[8..16].fill(0xff); // EOC marker

        let out: Buffer = buf.to_vec().into();
        let completion: Completion = {
            let mut ep = self.cmd_out.lock().await;
            ep.submit(out);
            ep.next_complete().await
        };
        completion.into_result().map_err(Error::Transfer)?;
        Ok(())
    }

    /// Send a batch of pucan commands (multiple 8-byte commands + 8-byte EOC).
    async fn send_cmd_batch(&self, cmds: &[[u8; 8]]) -> Result<(), Error> {
        let total = cmds.len() * 8 + 8; // cmds + EOC
        let mut buf = vec![0u8; total];
        for (i, c) in cmds.iter().enumerate() {
            buf[i * 8..i * 8 + 8].copy_from_slice(c);
        }
        buf[cmds.len() * 8..].fill(0xff); // EOC

        let out: Buffer = buf.into();
        let completion: Completion = {
            let mut ep = self.cmd_out.lock().await;
            ep.submit(out);
            ep.next_complete().await
        };
        completion.into_result().map_err(Error::Transfer)?;
        Ok(())
    }

    /// Set all 64 standard-filter rows to accept everything (mask = 0xFFFF_FFFF).
    /// Sends 8 batches of 8 commands (64 bytes each + EOC = 72 bytes per transfer).
    async fn set_filter_all_accept(&self) -> Result<(), Error> {
        for batch in 0..8usize {
            let cmds: Vec<[u8; 8]> = (0..8usize)
                .map(|j| cmd_filter_std((batch * 8 + j) as u16, 0xffff_ffffu32))
                .collect();
            self.send_cmd_batch(&cmds).await?;
        }
        Ok(())
    }

    /// Build and transmit a pucan TX message.
    async fn send_tx_msg(
        &self,
        can_id: u32,
        flags: u16,
        dlc: u8,
        data: &[u8],
    ) -> Result<(), Error> {
        // sizeof(pucan_tx_msg) without d[] = 2+2+4+4+1+1+2+4 = 20
        const HDR: usize = 20;
        let data_len = data.len();
        let msg_size = (HDR + data_len + 3) & !3; // ALIGN(HDR + data_len, 4)
        let total = msg_size + 4; // + null terminator (u32)

        let mut buf = vec![0u8; total];

        // size (le16)
        buf[0..2].copy_from_slice(&(msg_size as u16).to_le_bytes());
        // type (le16)
        buf[2..4].copy_from_slice(&PUCAN_MSG_CAN_TX.to_le_bytes());
        // tag_low, tag_high (le32 each) - zero
        // channel_dlc: channel=0 in bits[3:0], dlc in bits[7:4]
        buf[12] = dlc << 4; // channel 0
        // client: 0
        // flags (le16)
        buf[14..16].copy_from_slice(&flags.to_le_bytes());
        // can_id (le32)
        buf[16..20].copy_from_slice(&can_id.to_le_bytes());
        // data
        buf[HDR..HDR + data_len].copy_from_slice(data);
        // null terminator at msg_size: already zero

        let out: Buffer = buf.into();
        let completion: Completion = {
            let mut ep = self.data_out.lock().await;
            ep.submit(out);
            ep.next_complete().await
        };
        completion.into_result().map_err(Error::Transfer)?;
        Ok(())
    }

    async fn try_decode_one(&self) -> Option<CanMessage> {
        let mut buf = self.rx_buf.lock().await;
        decode_one(&mut buf)
    }
}

/// Simple command with only opcode_channel (no payload).
fn cmd_opcode(opcode: OpCode) -> [u8; 8] {
    let mut b = [0u8; 8];
    b[0..2].copy_from_slice(&opcode.as_le_bytes());
    b
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(u8)]
#[allow(unused)]
enum ClockMode {
    #[default]
    ClockMHz80 = 0x0,
    ClockMHz60 = 0x1,
    ClockMHz40 = 0x2,
    ClockMHz30 = 0x3,
    ClockMHz24 = 0x4,
    ClockMHz20 = 0x5,
}

impl ClockMode {
    fn as_hz(&self) -> u32 {
        match self {
            Self::ClockMHz80 => 80000000,
            Self::ClockMHz60 => 60000000,
            Self::ClockMHz40 => 40000000,
            Self::ClockMHz30 => 30000000,
            Self::ClockMHz24 => 24000000,
            Self::ClockMHz20 => 20000000,
        }
    }
}

/// Clock domain command.
fn cmd_clock(mode: ClockMode) -> [u8; 8] {
    let mut b = [0u8; 8];
    b[0..2].copy_from_slice(&OpCode::ClockSet.as_le_bytes());
    b[2] = mode as u8;
    b
}

/// LED command.
fn cmd_led(mode: u8) -> [u8; 8] {
    let mut b = [0u8; 8];
    b[0..2].copy_from_slice(&OpCode::LedSet.as_le_bytes());
    b[2] = mode;
    b
}

/// Write error counters command.
fn cmd_error_counters_write(tx: u8, rx: u8) -> [u8; 8] {
    let mut b = [0u8; 8];
    b[0..2].copy_from_slice(&OpCode::ErrorCounterWrite.as_le_bytes());
    b[2..4].copy_from_slice(&(PUCAN_WRERRCNT_TE | PUCAN_WRERRCNT_RE).to_le_bytes());
    b[4] = tx;
    b[5] = rx;
    b
}

/// Set or clear option flags.
fn cmd_option_set(opcode: OpCode, ucan_mask: u16, usb_mask: u16) -> [u8; 8] {
    // pcan_ufd_options: opcode_channel(2) + ucan_mask(2) + unused(2) + usb_mask(2)
    let mut b = [0u8; 8];
    b[0..2].copy_from_slice(&opcode.as_le_bytes());
    b[2..4].copy_from_slice(&ucan_mask.to_le_bytes());
    // b[4..6] unused = 0
    b[6..8].copy_from_slice(&usb_mask.to_le_bytes());
    b
}

/// FILTER_STD command - set row `idx` to `mask`.
fn cmd_filter_std(idx: u16, mask: u32) -> [u8; 8] {
    let mut b = [0u8; 8];
    b[0..2].copy_from_slice(&OpCode::FilterStandard.as_le_bytes());
    b[2..4].copy_from_slice(&idx.to_le_bytes());
    b[4..8].copy_from_slice(&mask.to_le_bytes());
    b
}

/// TIMING_SLOW command - nominal bittiming.
///
/// ```text
/// struct pucan_timing_slow {
///     __le16 opcode_channel;
///     u8     ewl;    // error warning limit
///     u8     sjw_t;  // SJW | (triple_sample << 7)
///     u8     tseg2;
///     u8     tseg1;
///     __le16 brp;
/// };
/// ```
fn cmd_timing_slow(bt: &BitTiming) -> [u8; 8] {
    let mut b = [0u8; 8];
    b[0..2].copy_from_slice(&OpCode::TimingSlow.as_le_bytes());
    b[2] = 96; // ewl default
    b[3] = (bt.sjw.saturating_sub(1)) & 0x7f; // sjw_t (triple sample bit = 0)
    b[4] = (bt.tseg2.saturating_sub(1)) & 0x7f;
    b[5] = ((bt.tseg1.saturating_sub(1)) & 0xff) as u8;
    b[6..8].copy_from_slice(&(bt.brp.saturating_sub(1)).to_le_bytes());
    b
}

/// TIMING_FAST command - CAN-FD data-phase bittiming.
///
/// ```text
/// struct pucan_timing_fast {
///     __le16 opcode_channel;
///     u8     unused;
///     u8     sjw;
///     u8     tseg2;
///     u8     tseg1;
///     __le16 brp;
/// };
/// ```
fn cmd_timing_fast(dbt: &DataBitTiming) -> [u8; 8] {
    let mut b = [0u8; 8];
    b[0..2].copy_from_slice(&OpCode::TimingFast.as_le_bytes());
    // b[2] unused
    b[3] = (dbt.sjw.saturating_sub(1)) & 0x0f;
    b[4] = (dbt.tseg2.saturating_sub(1)) & 0x0f;
    b[5] = (dbt.tseg1.saturating_sub(1)) & 0x1f;
    b[6..8].copy_from_slice(&(dbt.brp.saturating_sub(1)).to_le_bytes());
    b
}

// Byte offsets in a `pucan_rx_msg` (fixed header portion):
//
// ```text
// size(2) type(2) ts_low(4) ts_high(4) tag_low(4) tag_high(4)
// channel_dlc(1) client(1) flags(2) can_id(4) d[]
// ```
const RX_HDR: usize = 28; // sizeof(pucan_rx_msg) without d[]
const RX_OFF_SIZE: usize = 0;
const RX_OFF_TYPE: usize = 2;
const RX_OFF_CHANNEL_DLC: usize = 20;
const RX_OFF_FLAGS: usize = 22;
const RX_OFF_CAN_ID: usize = 24;
const RX_OFF_DATA: usize = 28;

// Byte offsets in a `pucan_status_msg` (20 bytes):
const ST_OFF_CHANNEL_P_W_B: usize = 12;

/// Decode one user-visible message from the accumulated receive buffer.
///
/// Walks the TLV records in the buffer (each record carries its own `size`
/// field). Records that don't produce a user-visible message (calibration
/// timestamps, busload, …) are consumed silently. Returns `None` when more
/// USB data is needed.
fn decode_one(buf: &mut Vec<u8>) -> Option<CanMessage> {
    loop {
        // Need at least the 4-byte message header (size + type).
        if buf.len() < 4 {
            return None;
        }

        let size = u16::from_le_bytes([buf[RX_OFF_SIZE], buf[RX_OFF_SIZE + 1]]) as usize;
        let msg_type = u16::from_le_bytes([buf[RX_OFF_TYPE], buf[RX_OFF_TYPE + 1]]);

        if size == 0 {
            // Null terminator - drain and stop.
            buf.drain(..buf.len());
            return None;
        }

        if buf.len() < size {
            // Wait for more data.
            return None;
        }

        let result = match msg_type {
            PUCAN_MSG_CAN_RX => decode_rx_msg(&buf[..size]),
            PUCAN_MSG_STATUS => decode_status_msg(&buf[..size]),
            // Calibration timestamps, overrun, busload - consume silently
            PCAN_UFD_MSG_CALIBRATION | PCAN_UFD_MSG_OVERRUN | PUCAN_MSG_ERROR => {
                None // consumed below
            }
            _ => None,
        };

        buf.drain(..size);

        if result.is_some() {
            return result;
        }
        // Otherwise loop and try the next record.
    }
}

fn decode_rx_msg(buf: &[u8]) -> Option<CanMessage> {
    if buf.len() < RX_HDR {
        return None;
    }

    let channel_dlc = buf[RX_OFF_CHANNEL_DLC];
    let dlc = channel_dlc >> 4;
    let flags = u16::from_le_bytes([buf[RX_OFF_FLAGS], buf[RX_OFF_FLAGS + 1]]);
    let can_id = u32::from_le_bytes([
        buf[RX_OFF_CAN_ID],
        buf[RX_OFF_CAN_ID + 1],
        buf[RX_OFF_CAN_ID + 2],
        buf[RX_OFF_CAN_ID + 3],
    ]);

    let id = if flags & PUCAN_MSG_EXT_ID != 0 {
        embedded_can::Id::Extended(
            embedded_can::ExtendedId::new(can_id & 0x1FFF_FFFF)
                .unwrap_or(embedded_can::ExtendedId::MAX),
        )
    } else {
        embedded_can::Id::Standard(
            embedded_can::StandardId::new((can_id & 0x7FF) as u16)
                .unwrap_or(embedded_can::StandardId::MAX),
        )
    };

    if flags & PUCAN_MSG_EXT_DATA_LEN != 0 {
        // CAN-FD frame
        let data_len = fd_dlc_to_len(dlc);
        if buf.len() < RX_OFF_DATA + data_len {
            return None;
        }
        let mut data = [0u8; 64];
        data[..data_len].copy_from_slice(&buf[RX_OFF_DATA..RX_OFF_DATA + data_len]);
        Some(CanMessage::FdFrame(FdFrame {
            id,
            data,
            len: data_len as u8,
            brs: flags & PUCAN_MSG_BITRATE_SWITCH != 0,
            esi: flags & PUCAN_MSG_ERROR_STATE_IND != 0,
        }))
    } else {
        // Classical CAN frame
        let rtr = flags & PUCAN_MSG_RTR != 0;
        let data_len = if rtr { 0 } else { dlc as usize };
        if buf.len() < RX_OFF_DATA + data_len {
            return None;
        }
        let mut data = [0u8; 8];
        data[..data_len].copy_from_slice(&buf[RX_OFF_DATA..RX_OFF_DATA + data_len]);
        Some(CanMessage::Frame(Frame { id, data, dlc, rtr }))
    }
}

fn decode_status_msg(buf: &[u8]) -> Option<CanMessage> {
    if buf.len() < 16 {
        return None;
    }
    let flags = buf[ST_OFF_CHANNEL_P_W_B];
    Some(CanMessage::Status(BusStatus {
        bus_off: flags & PUCAN_BUS_BUSOFF != 0,
        bus_warning: flags & PUCAN_BUS_WARNING != 0,
        bus_passive: flags & PUCAN_BUS_PASSIVE != 0,
        tx_err: 0,
        rx_err: 0,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_can::Frame as _;

    fn nominal_bitrate(bt: &BitTiming) -> u32 {
        let tq = 1u32 + bt.tseg1 as u32 + bt.tseg2 as u32;
        ClockMode::default().as_hz() / (bt.brp as u32 * tq)
    }

    fn data_bitrate(dbt: &DataBitTiming) -> u32 {
        let tq = 1u32 + dbt.tseg1 as u32 + dbt.tseg2 as u32;
        ClockMode::default().as_hz() / (dbt.brp as u32 * tq)
    }

    #[test]
    fn bittiming_1mbit() {
        let bt = BitTiming::from_bitrate(1_000_000);
        assert_eq!(nominal_bitrate(&bt), 1_000_000);
    }

    #[test]
    fn bittiming_500k() {
        let bt = BitTiming::from_bitrate(500_000);
        assert_eq!(nominal_bitrate(&bt), 500_000);
    }

    #[test]
    fn bittiming_250k() {
        let bt = BitTiming::from_bitrate(250_000);
        assert_eq!(nominal_bitrate(&bt), 250_000);
    }

    #[test]
    fn bittiming_125k() {
        let bt = BitTiming::from_bitrate(125_000);
        assert_eq!(nominal_bitrate(&bt), 125_000);
    }

    #[test]
    fn bittiming_50k() {
        let bt = BitTiming::from_bitrate(50_000);
        assert_eq!(nominal_bitrate(&bt), 50_000);
    }

    #[test]
    fn bittiming_20k() {
        let bt = BitTiming::from_bitrate(20_000);
        assert_eq!(nominal_bitrate(&bt), 20_000);
    }

    #[test]
    fn bittiming_10k() {
        let bt = BitTiming::from_bitrate(10_000);
        assert_eq!(nominal_bitrate(&bt), 10_000);
    }

    #[test]
    fn data_bittiming_2mbit() {
        let dbt = DataBitTiming::from_bitrate(2_000_000);
        assert_eq!(data_bitrate(&dbt), 2_000_000);
    }

    #[test]
    fn data_bittiming_5mbit() {
        let dbt = DataBitTiming::from_bitrate(5_000_000);
        assert_eq!(data_bitrate(&dbt), 5_000_000);
    }

    #[test]
    fn data_bittiming_8mbit() {
        let dbt = DataBitTiming::from_bitrate(8_000_000);
        assert_eq!(data_bitrate(&dbt), 8_000_000);
    }

    #[test]
    fn cmd_timing_slow_500k() {
        let bt = BitTiming::from_bitrate(500_000);
        let cmd = cmd_timing_slow(&bt);
        let oc = u16::from_le_bytes([cmd[0], cmd[1]]);
        assert_eq!(oc & 0x3ff, OpCode::TimingSlow as u16);
        let brp = u16::from_le_bytes([cmd[6], cmd[7]]) as u32 + 1;
        let tseg1 = cmd[5] as u32 + 1;
        let tseg2 = cmd[4] as u32 + 1;
        let actual = ClockMode::default().as_hz() / (brp * (1 + tseg1 + tseg2));
        assert_eq!(actual, 500_000);
    }

    #[test]
    fn cmd_timing_fast_2mbit() {
        let dbt = DataBitTiming::from_bitrate(2_000_000);
        let cmd = cmd_timing_fast(&dbt);
        let oc = u16::from_le_bytes([cmd[0], cmd[1]]);
        assert_eq!(oc & 0x3ff, OpCode::TimingFast as u16);
        let brp = u16::from_le_bytes([cmd[6], cmd[7]]) as u32 + 1;
        let tseg1 = cmd[5] as u32 + 1;
        let tseg2 = cmd[4] as u32 + 1;
        let actual = ClockMode::default().as_hz() / (brp * (1 + tseg1 + tseg2));
        assert_eq!(actual, 2_000_000);
    }

    #[test]
    fn cmd_opcode_reset() {
        let c = cmd_opcode(OpCode::ResetMode);
        let oc = u16::from_le_bytes([c[0], c[1]]);
        assert_eq!(oc, OpCode::ResetMode as u16);
    }

    /// Build a raw pucan_rx_msg packet for a classical CAN frame.
    fn make_rx_pkt(ext: bool, rtr: bool, can_id: u32, dlc: u8, data: &[u8]) -> Vec<u8> {
        let data_len = if rtr { 0 } else { data.len() };
        let size = (RX_HDR + data_len + 3) & !3;
        let mut pkt = vec![0u8; size];

        // size
        pkt[0..2].copy_from_slice(&(size as u16).to_le_bytes());
        // type = PUCAN_MSG_CAN_RX
        pkt[2..4].copy_from_slice(&PUCAN_MSG_CAN_RX.to_le_bytes());
        // channel_dlc: channel=0, dlc in high nibble
        pkt[RX_OFF_CHANNEL_DLC] = dlc << 4;
        // flags
        let mut flags: u16 = 0;
        if ext {
            flags |= PUCAN_MSG_EXT_ID;
        }
        if rtr {
            flags |= PUCAN_MSG_RTR;
        }
        pkt[RX_OFF_FLAGS..RX_OFF_FLAGS + 2].copy_from_slice(&flags.to_le_bytes());
        // can_id
        pkt[RX_OFF_CAN_ID..RX_OFF_CAN_ID + 4].copy_from_slice(&can_id.to_le_bytes());
        // data
        if !rtr && data_len > 0 {
            pkt[RX_OFF_DATA..RX_OFF_DATA + data_len].copy_from_slice(&data[..data_len]);
        }
        pkt
    }

    /// Build a raw pucan_rx_msg packet for a CAN-FD frame.
    fn make_fd_rx_pkt(ext: bool, can_id: u32, data: &[u8], brs: bool) -> Vec<u8> {
        let data_len = data.len();
        let dlc = fd_len_to_dlc(data_len);
        let size = (RX_HDR + data_len + 3) & !3;
        let mut pkt = vec![0u8; size];

        pkt[0..2].copy_from_slice(&(size as u16).to_le_bytes());
        pkt[2..4].copy_from_slice(&PUCAN_MSG_CAN_RX.to_le_bytes());
        pkt[RX_OFF_CHANNEL_DLC] = dlc << 4;
        let mut flags: u16 = PUCAN_MSG_EXT_DATA_LEN;
        if ext {
            flags |= PUCAN_MSG_EXT_ID;
        }
        if brs {
            flags |= PUCAN_MSG_BITRATE_SWITCH;
        }
        pkt[RX_OFF_FLAGS..RX_OFF_FLAGS + 2].copy_from_slice(&flags.to_le_bytes());
        pkt[RX_OFF_CAN_ID..RX_OFF_CAN_ID + 4].copy_from_slice(&can_id.to_le_bytes());
        pkt[RX_OFF_DATA..RX_OFF_DATA + data_len].copy_from_slice(data);
        pkt
    }

    #[test]
    fn decode_std_frame() {
        let mut buf = make_rx_pkt(false, false, 0x123, 4, &[0x11, 0x22, 0x33, 0x44]);
        let msg = decode_one(&mut buf).unwrap();
        match msg {
            CanMessage::Frame(f) => {
                assert_eq!(
                    f.id,
                    embedded_can::Id::Standard(embedded_can::StandardId::new(0x123).unwrap())
                );
                assert_eq!(f.dlc, 4);
                assert_eq!(&f.data[..4], &[0x11, 0x22, 0x33, 0x44]);
                assert!(!f.rtr);
            }
            _ => panic!("expected Frame, got {msg:?}"),
        }
    }

    #[test]
    fn decode_ext_frame() {
        let mut buf = make_rx_pkt(true, false, 0x1234_5678, 8, &[1, 2, 3, 4, 5, 6, 7, 8]);
        let msg = decode_one(&mut buf).unwrap();
        match msg {
            CanMessage::Frame(f) => {
                assert!(f.is_extended());
                assert_eq!(
                    f.id,
                    embedded_can::Id::Extended(embedded_can::ExtendedId::new(0x1234_5678).unwrap())
                );
                assert_eq!(f.dlc, 8);
            }
            _ => panic!("expected Frame"),
        }
    }

    #[test]
    fn decode_rtr_frame() {
        let mut buf = make_rx_pkt(false, true, 0x42, 4, &[]);
        match decode_one(&mut buf).unwrap() {
            CanMessage::Frame(f) => {
                assert!(f.rtr);
                assert_eq!(f.dlc, 4);
            }
            _ => panic!("expected Frame"),
        }
    }

    #[test]
    fn decode_fd_frame() {
        let data = (0u8..12).collect::<Vec<_>>();
        let mut buf = make_fd_rx_pkt(true, 0xABCD_EF01, &data, true);
        match decode_one(&mut buf).unwrap() {
            CanMessage::FdFrame(f) => {
                assert!(matches!(f.id, embedded_can::Id::Extended(_)));
                assert_eq!(f.len, 12);
                assert_eq!(&f.data[..12], &data[..]);
                assert!(f.brs);
            }
            _ => panic!("expected FdFrame"),
        }
    }

    #[test]
    fn decode_fd_frame_64() {
        let data = [0xCCu8; 64];
        let mut buf = make_fd_rx_pkt(false, 0x100, &data, false);
        match decode_one(&mut buf).unwrap() {
            CanMessage::FdFrame(f) => {
                assert_eq!(f.len, 64);
                assert_eq!(&f.data[..], &data[..]);
            }
            _ => panic!("expected FdFrame"),
        }
    }

    #[test]
    fn decode_status_busoff() {
        let mut buf = vec![0u8; 20];
        buf[0..2].copy_from_slice(&20u16.to_le_bytes());
        buf[2..4].copy_from_slice(&PUCAN_MSG_STATUS.to_le_bytes());
        buf[ST_OFF_CHANNEL_P_W_B] = PUCAN_BUS_BUSOFF;
        match decode_one(&mut buf).unwrap() {
            CanMessage::Status(s) => assert!(s.bus_off),
            _ => panic!("expected Status"),
        }
    }

    #[test]
    fn decode_status_warning() {
        let mut buf = vec![0u8; 20];
        buf[0..2].copy_from_slice(&20u16.to_le_bytes());
        buf[2..4].copy_from_slice(&PUCAN_MSG_STATUS.to_le_bytes());
        buf[ST_OFF_CHANNEL_P_W_B] = PUCAN_BUS_WARNING;
        match decode_one(&mut buf).unwrap() {
            CanMessage::Status(s) => {
                assert!(s.bus_warning);
                assert!(!s.bus_off);
            }
            _ => panic!("expected Status"),
        }
    }

    #[test]
    fn decode_calibration_skipped() {
        // A calibration message followed by a CAN frame - we should get the CAN frame.
        let mut buf = Vec::new();
        // Calibration msg (16 bytes)
        let mut cal = vec![0u8; 16];
        cal[0..2].copy_from_slice(&16u16.to_le_bytes());
        cal[2..4].copy_from_slice(&PCAN_UFD_MSG_CALIBRATION.to_le_bytes());
        buf.extend_from_slice(&cal);
        // CAN frame
        buf.extend_from_slice(&make_rx_pkt(false, false, 0x1, 1, &[0xAB]));

        let msg = decode_one(&mut buf).unwrap();
        assert!(matches!(msg, CanMessage::Frame(_)));
    }

    #[test]
    fn decode_null_size_terminates() {
        let mut buf = vec![0u8; 8]; // null size
        assert!(decode_one(&mut buf).is_none());
        assert!(buf.is_empty());
    }

    #[test]
    fn decode_returns_none_short_buf() {
        let mut buf = vec![0u8; 3]; // too short for even the header
        assert!(decode_one(&mut buf).is_none());
    }

    #[test]
    fn decode_consumes_bytes() {
        let mut buf = make_rx_pkt(false, false, 0x1, 1, &[0xFF]);
        let initial = buf.len();
        decode_one(&mut buf);
        assert!(buf.len() < initial);
    }

    #[test]
    fn cmd_filter_std_encoding() {
        let c = cmd_filter_std(7, 0xDEAD_BEEF);
        let oc = u16::from_le_bytes([c[0], c[1]]);
        assert_eq!(oc & 0x3ff, OpCode::FilterStandard as u16);
        let idx = u16::from_le_bytes([c[2], c[3]]);
        assert_eq!(idx, 7);
        let mask = u32::from_le_bytes([c[4], c[5], c[6], c[7]]);
        assert_eq!(mask, 0xDEAD_BEEF);
    }
}
