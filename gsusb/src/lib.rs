#![doc = include_str!("../README.md")]

mod frame;
mod protocol;

use futures::lock::Mutex;
use nusb::{
    DeviceInfo, Endpoint, Interface, MaybeFuture,
    transfer::{
        Buffer, Bulk, ControlIn, ControlOut, ControlType, In, Out, Recipient, TransferError,
    },
};
use protocol::*;
use std::time::Duration;
use zerocopy::{FromBytes, FromZeros, IntoBytes};

use crate::frame::ClassicCanTimestamp;
pub use crate::frame::Frame;

struct DeviceId {
    #[allow(unused)]
    name: &'static str,
    vendor_id: u16,
    product_id: u16,
}

impl DeviceId {
    const fn new(name: &'static str, vendor_id: u16, product_id: u16) -> Self {
        Self {
            name,
            vendor_id,
            product_id,
        }
    }
}

const DEVICES: &[DeviceId] = &[
    DeviceId::new("Geschwister Schneider", 0x1d50, 0x606f),
    DeviceId::new("CANdleLight", 0x1209, 0x2323),
    DeviceId::new("CES CANext FD", 0x1cd2, 0x606f),
    DeviceId::new("ABE CAN Debugger FD", 0x1cd2, 0x16d0),
    DeviceId::new("Xylanta Saint3", 0x16d0, 0x0f30),
];

/// GS USB errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("USB error: {0}")]
    Usb(#[from] nusb::Error),
    #[error("USB transfer error: {0}")]
    Transfer(#[from] TransferError),
    #[error("Command failed or malformed response from device: {0}")]
    Protocol(ProtocolErrorKind),
    #[error("Not a GS USB converter (product string mismatch)")]
    NotRecognized,
    #[error("Feature not supported by device")]
    FeatureNotSupported,
}

#[derive(Debug, thiserror::Error)]
pub enum ProtocolErrorKind {
    #[error("Response size invalid")]
    ResponseSize,
    #[error("Enum value invalid")]
    EnumValue,
}

/// List all connected GS USB devices.
pub async fn list_devices() -> Result<impl Iterator<Item = DeviceInfo>, Error> {
    let iter = nusb::list_devices()
        .await
        .map_err(Error::Usb)?
        .filter(|dev| {
            DEVICES
                .iter()
                .find(|d| dev.vendor_id() == d.vendor_id && dev.product_id() == d.product_id)
                .is_some()
        });
    Ok(iter)
}

pub struct Device {
    iface: Interface,
    data_tx: Mutex<Endpoint<Bulk, Out>>,
    data_rx: Mutex<Endpoint<Bulk, In>>,
}

impl Device {
    const TIMEOUT: Duration = Duration::from_millis(1000);

    pub async fn open(info: DeviceInfo) -> Result<Self, Error> {
        let _ = DEVICES
            .iter()
            .find(|d| d.vendor_id == info.vendor_id() && d.product_id == info.product_id())
            .ok_or(Error::NotRecognized)?;

        let device = info.open().await?;
        let _ = device.detach_kernel_driver(0); // try detach from kernel (only applicable to Linux)
        device.reset().await?;

        let iface: Interface = device.claim_interface(0).await?;
        let mut data_tx = iface.endpoint::<Bulk, Out>(0x02)?;
        let mut data_rx = iface.endpoint::<Bulk, In>(0x81)?;

        // A previous session may have exited with an in-flight transfer on one
        // of these pipes. On macOS, AbortPipe (called by transfer_blocking on
        // timeout or by cancel_all on drop) leaves the IOKit pipe in a halted
        // state. Any new transfer submitted to a halted pipe immediately
        // completes with kIOReturnAborted -> TransferError::Cancelled, which is
        // indistinguishable from a real cancellation. Clear both pipes
        // unconditionally here so we always start from a known-good state.
        let _ = data_tx.clear_halt().await;
        let _ = data_rx.clear_halt().await;

        Ok(Self {
            iface,
            data_tx: Mutex::new(data_tx),
            data_rx: Mutex::new(data_rx),
        })
    }

    /// Configure an channel.
    pub async fn start(&self, channel: u8, options: ChannelOptions) -> Result<(), Error> {
        let mut features = Features::new();
        if options.listen_only {
            features |= Features::FLAG_LISTEN_ONLY;
        }
        if options.loopback {
            features |= Features::FLAG_LOOP_BACK;
        }
        if options.triple_sample {
            features |= Features::FLAG_TRIPLE_SAMPLE;
        }
        if options.one_shot {
            features |= Features::FLAG_ONE_SHOT;
        }
        if options.hw_timestamp {
            features |= Features::FLAG_HW_TIMESTMAP;
        }
        if options.fd {
            features |= Features::FLAG_FD;
        }

        let mode = DeviceMode {
            mode: DeviceMode::MODE_START,
            flags: features,
        };

        self.iface
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    request: u8::from(ControlRequest::Mode),
                    value: u16::from(channel),
                    index: 0,
                    recipient: Recipient::Device,
                    data: mode.as_bytes(),
                },
                Self::TIMEOUT,
            )
            .await?;
        Ok(())
    }

    /// Reset a device channel.
    pub async fn reset(&self, channel: u8) -> Result<(), Error> {
        let mode = DeviceMode {
            mode: DeviceMode::MODE_RESET,
            flags: Features::new(),
        };

        self.iface
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    request: u8::from(ControlRequest::Mode),
                    value: u16::from(channel),
                    index: 0,
                    recipient: Recipient::Device,
                    data: mode.as_bytes(),
                },
                Self::TIMEOUT,
            )
            .await?;
        Ok(())
    }

    /// Get device bit timing parameters.
    fn get_bit_timing_const(
        &self,
        channel: u8,
    ) -> impl MaybeFuture<Output = Result<DeviceBitTimingConst, Error>> {
        self.iface
            .control_in(
                ControlIn {
                    control_type: ControlType::Vendor,
                    request: u8::from(ControlRequest::BitTimingConst),
                    value: u16::from(channel),
                    length: std::mem::size_of::<DeviceBitTimingConst>() as u16,
                    index: 0,
                    recipient: Recipient::Device,
                },
                Self::TIMEOUT,
            )
            .map(|r| {
                DeviceBitTimingConst::read_from_bytes(&r.unwrap())
                    .map_err(|_| Error::Protocol(ProtocolErrorKind::ResponseSize))
            })
    }

    /// Set channel bitrate.
    pub async fn bitrate(&self, channel: u8, bitrate: u32) -> Result<(), Error> {
        let bit_timing_const = self.get_bit_timing_const(channel).await?;
        let timing =
            timing_from_bitrate(bitrate, &bit_timing_const.timing, bit_timing_const.fclk_can);
        Ok(self
            .iface
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    request: u8::from(ControlRequest::BitTiming),
                    value: u16::from(channel),
                    index: 0,
                    recipient: Recipient::Device,
                    data: timing.as_bytes(),
                },
                Self::TIMEOUT,
            )
            .await?)
    }

    /// Get channel state.
    pub async fn state(&self, channel: u8) -> Result<State, Error> {
        let response = self
            .iface
            .control_in(
                ControlIn {
                    control_type: ControlType::Vendor,
                    request: u8::from(ControlRequest::GetState),
                    value: u16::from(channel),
                    length: std::mem::size_of::<DeviceState>() as u16,
                    index: 0,
                    recipient: Recipient::Device,
                },
                Self::TIMEOUT,
            )
            .await?;
        let device_state = DeviceState::ref_from_bytes(&response)
            .map_err(|_| Error::Protocol(ProtocolErrorKind::ResponseSize))?;
        Ok(State {
            state: device_state
                .state()
                .ok_or(Error::Protocol(ProtocolErrorKind::EnumValue))?,
            rx_errors: device_state.rx_errors,
            tx_errors: device_state.tx_errors,
        })
    }

    /// Send a frame on the channel.
    pub async fn send(&self, channel: u8, frame: &Frame) -> Result<(), Error> {
        let mut frame = frame.to_owned();
        frame.channel = channel;

        // only fd frames are over 64 byte and use two transfers.
        let bytes = if frame.flags.fd() {
            frame.as_bytes()
        } else {
            &frame.as_bytes()[..ClassicCanTimestamp::PADDING]
        };
        let buffer = Buffer::from(bytes.to_vec());

        let completion = {
            let mut ep = self.data_tx.lock().await;
            ep.submit(buffer);
            ep.next_complete().await
        };
        completion.into_result().map_err(Error::Transfer)?;
        Ok(())
    }

    /// Receive a frame.
    pub async fn recv(&self) -> Result<Frame, Error> {
        let size = std::mem::size_of::<Frame>().next_multiple_of(64);
        let rx = Buffer::new(size);
        let completion = {
            let mut ep = self.data_rx.lock().await;
            ep.submit(rx);
            ep.next_complete().await
        };
        let buf = completion.into_result()?;
        let mut frame = Frame::new_zeroed();
        frame.as_mut_bytes()[..buf.len()].clone_from_slice(&buf);
        Ok(frame)
    }
}

/// Calculate the best approximation for bit timing values.
pub fn timing_from_bitrate(bitrate: u32, timing: &CanBitTimingConst, fclk: u32) -> DeviceBitTiming {
    let mut best = DeviceBitTiming {
        prop_seg: 1,
        phase_seg1: 13,
        phase_seg2: 2,
        sjw: 1,
        brp: 4,
    };
    let mut best_err = u64::MAX;

    for brp in timing.brp_min..=timing.brp_max {
        for tseg1 in timing.tseg1_min..=timing.tseg1_max {
            for tseg2 in timing.tseg2_min..=timing.tseg2_max {
                let tq = 1 + tseg1 + tseg2;
                let actual = fclk / (brp * tq);
                let err = (actual as i64 - bitrate as i64).unsigned_abs();
                if err < best_err {
                    best_err = err;
                    best = DeviceBitTiming {
                        prop_seg: 1,
                        phase_seg1: tseg1,
                        phase_seg2: tseg2,
                        sjw: tseg2.min(4),
                        brp,
                    };
                }
            }
        }
    }

    best
}

/// Interface configuration options
#[derive(Debug, Clone, Copy, Default)]
pub struct ChannelOptions {
    /// Do not transmit frames.
    pub listen_only: bool,
    /// Receive sent frames.
    pub loopback: bool,
    /// Sample 3 time quanta instead of 1 for better signal integrity.
    pub triple_sample: bool,
    /// Don't retry transmission.
    pub one_shot: bool,
    /// Hardware timestamp for received frames.
    pub hw_timestamp: bool,
    /// Receive FD frames.
    pub fd: bool,
}

/// Channel state and error counters.
#[derive(Debug, Clone, Copy)]
pub struct State {
    pub state: CanState,
    /// Receive error count.
    pub rx_errors: u32,
    /// Transmit error count.
    pub tx_errors: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timing_from_bitrate() {
        let target = 500_000;
        let timing_const = CanBitTimingConst {
            tseg1_min: 1,
            tseg1_max: 255,
            tseg2_min: 1,
            tseg2_max: 127,
            sjw_max: 127,
            brp_min: 1,
            brp_max: 511,
            brp_inc: 1,
        };
        let timing = timing_from_bitrate(target, &timing_const, 8_000_000);
        assert_eq!(timing.phase_seg1, 1);
        assert_eq!(timing.phase_seg2, 14);
        assert_eq!(timing.prop_seg, 1);
        assert_eq!(timing.sjw, 4);
        assert_eq!(timing.brp, 1);
    }
}
