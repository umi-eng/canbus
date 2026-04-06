#![allow(unused)]

use std::ops::BitOrAssign;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// USB control request types.
#[derive(Debug)]
pub enum ControlRequest {
    HostFormat = 0,
    BitTiming = 1,
    Mode = 2,
    BusError = 3,
    BitTimingConst = 4,
    DeviceConfig = 5,
    Timestamp = 6,
    Identify = 7,
    GetUserId = 8,
    SetuserId = 9,
    DataBitTiming = 10,
    BitTimingConstExt = 11,
    SetTermination = 12,
    GetTermination = 13,
    GetState = 14,
}

impl From<ControlRequest> for u8 {
    fn from(value: ControlRequest) -> Self {
        value as u8
    }
}

/// Host configuration.
///
/// Indicates byte order with a magic number. `0x0000beef` for little endian and
/// `0xefbe0000` for big endian.
#[derive(Debug, IntoBytes)]
#[repr(C)]
pub struct HostConfig {
    pub(crate) byte_order: u32,
}

/// Device configuration.
#[derive(Debug, FromBytes)]
#[repr(C)]
pub struct DeviceConfig {
    _reserved0: u8,
    _reserved1: u8,
    _reserved2: u8,
    pub(crate) interface_count: u8,
    pub(crate) software_version: u32,
    pub(crate) hardware_version: u32,
}

/// Device mode and feature flags.
#[derive(Debug, FromBytes, IntoBytes, Immutable)]
#[repr(C)]
pub struct DeviceMode {
    pub(crate) mode: u32,
    pub(crate) flags: Features,
}

impl DeviceMode {
    pub const MODE_RESET: u32 = 0;
    pub const MODE_START: u32 = 1;
}

/// Device features and modes.
#[derive(Debug, FromBytes, IntoBytes, Immutable)]
pub struct Features(u32);

impl Features {
    pub const FLAG_LISTEN_ONLY: Self = Self(1 << 0);
    pub const FLAG_LOOP_BACK: Self = Self(1 << 1);
    pub const FLAG_TRIPLE_SAMPLE: Self = Self(1 << 2);
    pub const FLAG_ONE_SHOT: Self = Self(1 << 3);
    pub const FLAG_HW_TIMESTMAP: Self = Self(1 << 4);
    pub const FLAG_IDENTIFY: Self = Self(1 << 5);
    pub const FLAG_USER_ID: Self = Self(1 << 6);
    pub const FLAG_PAD_PKTS_TO_MAX_SIZE: Self = Self(1 << 7);
    pub const FLAG_FD: Self = Self(1 << 8);
    pub const FLAG_USB_QUIRK_LPC546XX: Self = Self(1 << 9);
    pub const FLAG_BT_CONST_EXT: Self = Self(1 << 10);
    pub const FLAG_TERMINATION: Self = Self(1 << 11);
    pub const FLAG_BUS_ERROR_REPORTING: Self = Self(1 << 12);
    pub const FLAG_GET_STATE: Self = Self(1 << 13);

    pub fn new() -> Self {
        Features(0)
    }
}

impl BitOrAssign for Features {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Debug, FromBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct DeviceState {
    pub state: u32,
    pub rx_errors: u32,
    pub tx_errors: u32,
}

impl DeviceState {
    pub fn state(&self) -> Option<CanState> {
        match self.state {
            0 => Some(CanState::Active),
            1 => Some(CanState::Warning),
            2 => Some(CanState::Passive),
            3 => Some(CanState::BusOff),
            4 => Some(CanState::Stopped),
            5 => Some(CanState::Sleeping),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanState {
    /// RX/TX error count < 96
    Active,
    /// RX/TX error count < 128
    Warning,
    /// RX/TX error count < 256
    Passive,
    /// RX/TX error count >= 256
    BusOff,
    /// Device is stopped
    Stopped,
    /// Device is sleeping
    Sleeping,
}

#[derive(Debug, IntoBytes, Immutable)]
#[repr(C)]
pub struct DeviceBitTiming {
    pub prop_seg: u32,
    pub phase_seg1: u32,
    pub phase_seg2: u32,
    pub sjw: u32,
    pub brp: u32,
}

#[derive(Debug, IntoBytes, FromBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct CanBitTimingConst {
    pub tseg1_min: u32,
    pub tseg1_max: u32,
    pub tseg2_min: u32,
    pub tseg2_max: u32,
    pub sjw_max: u32,
    pub brp_min: u32,
    pub brp_max: u32,
    pub brp_inc: u32,
}

/// Device bit-timing and feature flags.
#[derive(Debug, IntoBytes, FromBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct DeviceBitTimingConst {
    pub features: Features,
    pub fclk_can: u32,
    pub timing: CanBitTimingConst,
}

/// Device extended bit-timing and feature flags for CAN FD devices.
#[derive(Debug, IntoBytes, FromBytes)]
#[repr(C)]
pub struct DeviceBitTimingConstExtended {
    pub features: Features,
    pub fclk_can: u32,
    pub timing_nominal: CanBitTimingConst,
    pub timing_data: CanBitTimingConst,
}

#[derive(Debug, FromBytes, IntoBytes)]
#[repr(C)]
pub struct IdentifyMode {
    pub mode: u32,
}

#[derive(Debug, FromBytes, IntoBytes)]
#[repr(C)]
pub struct DeviceTerminationState {
    pub state: u32,
}
