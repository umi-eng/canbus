/// Implement bitwise opererations for newtypes that wrap scalars.
macro_rules! impl_bitwise_ops {
    ($type:ident) => {
        impl ::core::ops::BitAnd for $type {
            type Output = Self;

            fn bitand(self, rhs: Self) -> Self::Output {
                Self(self.0 & rhs.0)
            }
        }

        impl ::core::ops::BitOr for $type {
            type Output = Self;

            fn bitor(self, rhs: Self) -> Self::Output {
                Self(self.0 | rhs.0)
            }
        }

        impl ::core::ops::BitXor for $type {
            type Output = Self;

            fn bitxor(self, rhs: Self) -> Self::Output {
                Self(self.0 ^ rhs.0)
            }
        }

        impl ::core::ops::BitAndAssign for $type {
            fn bitand_assign(&mut self, rhs: Self) {
                *self = Self(self.0 & rhs.0)
            }
        }

        impl ::core::ops::BitOrAssign for $type {
            fn bitor_assign(&mut self, rhs: Self) {
                *self = Self(self.0 | rhs.0)
            }
        }

        impl ::core::ops::BitXorAssign for $type {
            fn bitxor_assign(&mut self, rhs: Self) {
                *self = Self(self.0 ^ rhs.0)
            }
        }
    };
}

/// Error frame details.
#[derive(Debug, Default, Clone, Copy)]
pub struct CanError {
    /// Transmit timeout.
    pub tx_timeout: bool,
    /// Arbitration lost with bit number in bit stream. Zero if unspecified.
    pub lost_arbitration: Option<u8>,
    /// Controller error.
    pub controller: Option<ControllerError>,
    /// Protocol error kind and location.
    pub protocol: Option<(ProtocolErrorKind, Result<ProtocolErrorLocation, u8>)>,
    /// Transceiver error.
    pub transceiver: Option<TransceiverError>,
    /// No ack received.
    pub no_ack: bool,
    /// Bus-off state.
    pub bus_off: bool,
    /// Bus-error state.
    pub bus_error: bool,
    /// Controller restarted.
    pub restarted: bool,
    /// Transmit/receive error count.
    pub tx_rx_error_count: Option<(u8, u8)>,
}

/// Controller error state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControllerError(pub(crate) u8);

impl_bitwise_ops!(ControllerError);

impl ControllerError {
    /// Unspecified error.
    pub const UNSPECIFIED: Self = Self(0);
    /// Receive mailbox overflow.
    pub const RX_OVERFLOW: Self = Self(libc::CAN_ERR_CRTL_RX_OVERFLOW as u8);
    /// Transmit mailbox overflow.
    pub const TX_OVERFLOW: Self = Self(libc::CAN_ERR_CRTL_TX_OVERFLOW as u8);
    /// Receive mailbox warning level reached.
    pub const RX_WARNING: Self = Self(libc::CAN_ERR_CRTL_RX_WARNING as u8);
    /// Transmit mailbox warning level reached.
    pub const TX_WARNING: Self = Self(libc::CAN_ERR_CRTL_TX_WARNING as u8);
    /// Receive mailbox passive level reached.
    pub const RX_PASSIVE: Self = Self(libc::CAN_ERR_CRTL_RX_PASSIVE as u8);
    /// Transmit mailbox passive level reached.
    pub const TX_PASSIVE: Self = Self(libc::CAN_ERR_CRTL_TX_PASSIVE as u8);
    /// Controller recovered to active state.
    pub const ACTIVE: Self = Self(libc::CAN_ERR_CRTL_ACTIVE as u8);
}

/// Protocol error kind flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolErrorKind(pub(crate) u8);

impl_bitwise_ops!(ProtocolErrorKind);

impl ProtocolErrorKind {
    /// Unspecified error.
    pub const UNSPECIFIED: Self = Self(0);
    /// Bit error.
    pub const BIT: Self = Self(libc::CAN_ERR_PROT_BIT as u8);
    /// Form error.
    pub const FORM: Self = Self(libc::CAN_ERR_PROT_FORM as u8);
    /// Bit-stuffing error.
    pub const STUFF: Self = Self(libc::CAN_ERR_PROT_STUFF as u8);
    /// Bit 0 (dominant) error.
    pub const BIT_DOMINANT: Self = Self(libc::CAN_ERR_PROT_BIT0 as u8);
    /// Bit 1 (recessive) error.
    pub const BIT_RECESSIVE: Self = Self(libc::CAN_ERR_PROT_BIT1 as u8);
    /// Bus overload error.
    pub const OVERLOAD: Self = Self(libc::CAN_ERR_PROT_OVERLOAD as u8);
    /// Active error.
    pub const ACTIVE: Self = Self(libc::CAN_ERR_PROT_ACTIVE as u8);
    /// Transmit error.
    pub const TRANSMIT: Self = Self(libc::CAN_ERR_PROT_TX as u8);
}

/// Protocol error location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolErrorLocation {
    /// Unspecified.
    Unspecified = 0x00,
    /// Start of frame.
    Sof = 0x03,
    /// Bits 28 - 21.
    Id28_21 = 0x02,
    /// Bits 20 - 18.
    Id20_18 = 0x06,
    /// Substitude RTR.
    Srtr = 0x04,
    /// Identifier extension.
    Ide = 0x05,
    /// Bits 17 - 13.
    Id17_13 = 0x07,
    /// Bits 12 - 5.
    Id12_5 = 0x0F,
    /// Bits 4 - 0.
    Id4_0 = 0x0E,
    /// RTR bit.
    Rtr = 0x0C,
    /// Reserved bit 1.
    Res1 = 0x0D,
    /// Reserved bit 0.
    Res0 = 0x09,
    /// Data length code.
    Dlc = 0x0B,
    /// Data section.
    Data = 0x0A,
    /// CRC sequence.
    CrcSeq = 0x08,
    /// CRC delimiter.
    CrcDel = 0x18,
    /// ACK slot.
    Ack = 0x19,
    /// ACK delimiter.
    AckDel = 0x1B,
    /// End of frame.
    Eof = 0x1A,
    /// Intermission.
    Int = 0x12,
}

impl TryFrom<u8> for ProtocolErrorLocation {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(Self::Unspecified),
            0x03 => Ok(Self::Sof),
            0x02 => Ok(Self::Id28_21),
            0x06 => Ok(Self::Id20_18),
            0x04 => Ok(Self::Srtr),
            0x05 => Ok(Self::Ide),
            0x07 => Ok(Self::Id17_13),
            0x0F => Ok(Self::Id12_5),
            0x0E => Ok(Self::Id4_0),
            0x0C => Ok(Self::Rtr),
            0x0D => Ok(Self::Res1),
            0x09 => Ok(Self::Res0),
            0x0B => Ok(Self::Dlc),
            0x0A => Ok(Self::Data),
            0x08 => Ok(Self::CrcSeq),
            0x18 => Ok(Self::CrcDel),
            0x19 => Ok(Self::Ack),
            0x1B => Ok(Self::AckDel),
            0x1A => Ok(Self::Eof),
            0x12 => Ok(Self::Int),
            _ => Err(value),
        }
    }
}

/// Transceiver error.
#[derive(Debug, Clone, Copy)]
pub struct TransceiverError(pub(crate) u8);

impl TransceiverError {
    /// CAN high error.
    pub fn error_high(&self) -> Option<TransceiverErrorKind> {
        match self.0 {
            x if x == libc::CAN_ERR_TRX_UNSPEC as u8 => Some(TransceiverErrorKind::Unspecified),
            x if x == libc::CAN_ERR_TRX_CANH_NO_WIRE as u8 => Some(TransceiverErrorKind::NoWire),
            x if x == libc::CAN_ERR_TRX_CANH_SHORT_TO_BAT as u8 => {
                Some(TransceiverErrorKind::ShortToBattery)
            }
            x if x == libc::CAN_ERR_TRX_CANH_SHORT_TO_VCC as u8 => {
                Some(TransceiverErrorKind::ShortToPower)
            }
            x if x == libc::CAN_ERR_TRX_CANH_SHORT_TO_GND as u8 => {
                Some(TransceiverErrorKind::ShortToGround)
            }
            x if x == libc::CAN_ERR_TRX_CANL_SHORT_TO_CANH as u8 => {
                Some(TransceiverErrorKind::ShortLowToHigh)
            }
            _ => None,
        }
    }

    /// CAN low error.
    pub fn error_low(&self) -> Option<TransceiverErrorKind> {
        match self.0 {
            x if x == libc::CAN_ERR_TRX_UNSPEC as u8 => Some(TransceiverErrorKind::Unspecified),
            x if x == libc::CAN_ERR_TRX_CANL_NO_WIRE as u8 => Some(TransceiverErrorKind::NoWire),
            x if x == libc::CAN_ERR_TRX_CANL_SHORT_TO_BAT as u8 => {
                Some(TransceiverErrorKind::ShortToBattery)
            }
            x if x == libc::CAN_ERR_TRX_CANL_SHORT_TO_VCC as u8 => {
                Some(TransceiverErrorKind::ShortToPower)
            }
            x if x == libc::CAN_ERR_TRX_CANL_SHORT_TO_GND as u8 => {
                Some(TransceiverErrorKind::ShortToGround)
            }
            x if x == libc::CAN_ERR_TRX_CANL_SHORT_TO_CANH as u8 => {
                Some(TransceiverErrorKind::ShortLowToHigh)
            }
            _ => None,
        }
    }

    pub fn error_unspecified(&self) -> bool {
        self.0 == libc::CAN_ERR_TRX_UNSPEC as u8
    }

    pub fn error_short_low_to_high(&self) -> bool {
        self.0 == libc::CAN_ERR_TRX_CANL_SHORT_TO_CANH as u8
    }
}

/// Transceiver error type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TransceiverErrorKind {
    /// Unspecified error.
    Unspecified,
    /// Not connected.
    NoWire,
    /// Short to battery.
    ShortToBattery,
    /// Short to power.
    ShortToPower,
    /// Short to ground.
    ShortToGround,
    /// Short low to high.
    ShortLowToHigh,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_error_location_unknown() {
        assert_eq!(ProtocolErrorLocation::try_from(255), Err(255));
    }

    #[test]
    fn protocol_error_location_unspecified() {
        assert_eq!(ProtocolErrorLocation::Unspecified as u8, 0);
    }

    #[test]
    fn tranceiver_error_unspecified() {
        assert!(TransceiverError(libc::CAN_ERR_TRX_UNSPEC as u8).error_unspecified());
        assert!(TransceiverError(libc::CAN_ERR_TRX_UNSPEC as u8).error_short_low_to_high() != true);
        assert_eq!(
            TransceiverError(libc::CAN_ERR_TRX_UNSPEC as u8).error_high(),
            Some(TransceiverErrorKind::Unspecified)
        );
        assert_eq!(
            TransceiverError(libc::CAN_ERR_TRX_UNSPEC as u8).error_low(),
            Some(TransceiverErrorKind::Unspecified)
        );
    }

    #[test]
    fn transceiver_error_variants() {
        assert_eq!(
            TransceiverError(libc::CAN_ERR_TRX_CANH_NO_WIRE as u8).error_high(),
            Some(TransceiverErrorKind::NoWire)
        );
        assert_eq!(
            TransceiverError(libc::CAN_ERR_TRX_CANH_SHORT_TO_BAT as u8).error_high(),
            Some(TransceiverErrorKind::ShortToBattery)
        );
        assert_eq!(
            TransceiverError(libc::CAN_ERR_TRX_CANH_SHORT_TO_VCC as u8).error_high(),
            Some(TransceiverErrorKind::ShortToPower)
        );
        assert_eq!(
            TransceiverError(libc::CAN_ERR_TRX_CANH_SHORT_TO_GND as u8).error_high(),
            Some(TransceiverErrorKind::ShortToGround)
        );

        assert_eq!(
            TransceiverError(libc::CAN_ERR_TRX_CANH_NO_WIRE as u8).error_high(),
            Some(TransceiverErrorKind::NoWire)
        );
        assert_eq!(
            TransceiverError(libc::CAN_ERR_TRX_CANH_SHORT_TO_BAT as u8).error_high(),
            Some(TransceiverErrorKind::ShortToBattery)
        );
        assert_eq!(
            TransceiverError(libc::CAN_ERR_TRX_CANH_SHORT_TO_VCC as u8).error_high(),
            Some(TransceiverErrorKind::ShortToPower)
        );
        assert_eq!(
            TransceiverError(libc::CAN_ERR_TRX_CANH_SHORT_TO_GND as u8).error_high(),
            Some(TransceiverErrorKind::ShortToGround)
        );
    }

    #[test]
    fn transceiver_error_invalid() {
        assert_eq!(TransceiverError(0xFF).error_high(), None);
        assert_eq!(TransceiverError(0xFF).error_low(), None);
        assert_eq!(TransceiverError(0xFF).error_unspecified(), false);
        assert_eq!(TransceiverError(0xFF).error_short_low_to_high(), false);
    }
}
