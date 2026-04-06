#![allow(unused)]

use std::fmt::Debug;
use embedded_can::{ExtendedId, Id, StandardId};
use zerocopy::{FromBytes, FromZeros, Immutable, IntoBytes};

#[derive(Clone, Copy, FromBytes, IntoBytes, Immutable)]
#[repr(C)]
pub struct Frame {
    pub(crate) echo_id: u32,
    pub(crate) can_id: u32,
    pub(crate) can_dlc: u8,
    pub(crate) channel: u8,
    pub(crate) flags: FrameFlags,
    _reserved0: u8,
    pub(crate) can_data: CanData,
}

impl Frame {
    const EFF_FLAG: u32 = 0x80000000;
    const RTR_FLAG: u32 = 0x40000000;
    const ERR_FLAG: u32 = 0x20000000;
    const SFF_MASK: u32 = 0x000007FF;
    const EFF_MASK: u32 = 0x1FFFFFFF;
    const ERR_MASK: u32 = 0x1FFFFFFF;

    /// Channel this frame was received on.
    ///
    /// Only valid for frames received on a device. Constructed frames always
    /// have the channel 0.
    fn channel(&self) -> u8 {
        self.channel
    }
}

impl Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let data = unsafe { &self.can_data.can_fd.data[..self.can_dlc as usize] };
        f.debug_struct("Frame")
            .field("echo_id", &self.echo_id)
            .field("can_id", &self.can_id)
            .field("can_dlc", &self.can_dlc)
            .field("interface", &self.channel)
            .field("flags", &self.flags)
            .field("can_data", &data)
            .finish()
    }
}

impl embedded_can::Frame for Frame {
    fn new(id: impl Into<Id>, data: &[u8]) -> Option<Self> {
        let mut frame = Frame::new_zeroed();
        frame.can_dlc = data.len() as u8;
        frame.can_id = match id.into() {
            Id::Standard(id) => id.as_raw() as u32,
            Id::Extended(id) => id.as_raw() | Self::EFF_FLAG,
        };
        unsafe { frame.can_data.classic_can.data[..data.len()].copy_from_slice(data) };

        Some(frame)
    }

    fn new_remote(id: impl Into<Id>, dlc: usize) -> Option<Self> {
        let mut frame = Frame::new_zeroed();
        frame.can_dlc = dlc as u8;
        frame.can_id = match id.into() {
            Id::Standard(id) => id.as_raw() as u32,
            Id::Extended(id) => id.as_raw() | Self::EFF_FLAG,
        };
        frame.can_id |= Self::RTR_FLAG;
        Some(frame)
    }

    fn id(&self) -> Id {
        let masked = self.can_id & Self::EFF_MASK;
        if self.is_extended() {
            Id::Extended(ExtendedId::new(masked).unwrap())
        } else {
            Id::Standard(StandardId::new(masked as u16).unwrap())
        }
    }

    fn is_extended(&self) -> bool {
        (self.can_id & Self::EFF_FLAG) != 0
    }

    fn is_remote_frame(&self) -> bool {
        (self.can_id & Self::RTR_FLAG) != 0
    }

    fn dlc(&self) -> usize {
        self.can_dlc as usize
    }

    fn data(&self) -> &[u8] {
        // safety: underlying type is initialised with zeros and length is given by dlc.
        if self.flags.fd() {
            unsafe { &self.can_data.can_fd.data[..self.dlc()] }
        } else {
            unsafe { &self.can_data.classic_can.data[..self.dlc()] }
        }
    }
}

/// Frame flags.
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable)]
pub(crate) struct FrameFlags(u8);

impl FrameFlags {
    const OVERFLOW_FLAG: u8 = 0x1;
    const BRS_FLAG: u8 = 0x2;
    const FDF_FLAG: u8 = 0x4;
    const ESI_FLAG: u8 = 0x8;

    /// Overflow.
    #[allow(unused)]
    pub fn overflow(&self) -> bool {
        (self.0 & Self::OVERFLOW_FLAG) != 0
    }

    /// Bitrate switching.
    #[allow(unused)]
    pub fn brs(&self) -> bool {
        (self.0 & Self::BRS_FLAG) != 0
    }

    /// FD frame.
    pub fn fd(&self) -> bool {
        (self.0 & Self::FDF_FLAG) != 0
    }

    /// Error state indicator flag.
    #[allow(unused)]
    pub fn esi(&self) -> bool {
        (self.0 & Self::ESI_FLAG) != 0
    }
}

#[derive(Clone, Copy, FromBytes, IntoBytes, Immutable)]
#[repr(C)]
pub(crate) union CanData {
    pub classic_can: ClassicCan,
    pub classic_can_timestamp: ClassicCanTimestamp,
    pub can_fd: CanFd,
    pub can_fd_timestamp: CanFdTimestamp,
}

#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable)]
#[repr(C)]
pub(crate) struct ClassicCan {
    pub data: [u8; 8],
    _padding: [u8; 60],
}

#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable)]
#[repr(C)]
pub(crate) struct ClassicCanTimestamp {
    pub data: [u8; 8],
    pub timestamp_us: u32,
    _padding: [u8; 56],
}

#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable)]
#[repr(C)]
pub(crate) struct CanFd {
    pub data: [u8; 64],
    _padding: [u8; 4],
}

#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable)]
#[repr(C)]
pub(crate) struct CanFdTimestamp {
    pub data: [u8; 64],
    pub timestamp_us: u32,
}
