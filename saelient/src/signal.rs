//! Signals (J1939-71)

/// Signal type.
pub trait Signal: Sized {
    /// Underlying base type.
    type Base: num::FromPrimitive + num::cast::AsPrimitive<u32>;

    /// Create from raw value.
    ///
    /// Returns `None` if the value provided is greater than the maximum
    /// allowable for the signal.
    fn from_raw(value: Self::Base) -> Option<Self>;

    /// Get the raw value.
    fn to_raw(&self) -> Self::Base;

    /// Inner value if valid.
    fn value(&self) -> Option<Self::Base>;

    /// Signal is valid.
    fn is_valid(&self) -> bool;

    /// Get the indicator value if it is present.
    fn indicator(&self) -> Option<Self::Base>;

    /// Signal is a parameter specific indicator.
    fn is_indicator(&self) -> bool;

    /// Get the error value if it is present.
    fn error(&self) -> Option<Self::Base>;

    /// Signal indicates an error.
    fn is_error(&self) -> bool;

    /// Get the error value if it is present.
    fn not_present(&self) -> Option<Self::Base>;

    /// Signal is not available or was not requested.
    fn is_not_present(&self) -> bool;
}

macro_rules! signal_impl {
    (
        $type:ident, $base:ty,
        valid: $valid_min:literal ..= $valid_max:literal,
        indicator: $indicator_min:literal ..= $indicator_max:literal,
        error: $error_min:literal ..= $error_max:literal,
        not_present: $not_present_min:literal ..= $not_present_max:literal $(,)?
    ) => {
        /// Parameter signal.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[cfg_attr(feature = "defmt-1", derive(defmt::Format))]
        pub struct $type($base);

        impl Signal for $type {
            type Base = $base;

            fn from_raw(value: $base) -> Option<Self> {
                match value {
                    $valid_min..=$valid_max
                    | $indicator_min..=$indicator_max
                    | $error_min..=$error_max
                    | $not_present_min..=$not_present_max => Some(Self(value)),
                    _ => None,
                }
            }

            fn to_raw(&self) -> $base {
                self.0
            }

            fn value(&self) -> Option<Self::Base> {
                if self.is_valid() { Some(self.0) } else { None }
            }

            fn is_valid(&self) -> bool {
                match self.0 {
                    $valid_min..=$valid_max => true,
                    _ => false,
                }
            }

            fn indicator(&self) -> Option<Self::Base> {
                if self.is_indicator() {
                    Some(self.0)
                } else {
                    None
                }
            }

            fn is_indicator(&self) -> bool {
                match self.0 {
                    $indicator_min..=$indicator_max => true,
                    _ => false,
                }
            }

            fn error(&self) -> Option<Self::Base> {
                if self.is_error() { Some(self.0) } else { None }
            }

            fn is_error(&self) -> bool {
                match self.0 {
                    $error_min..=$error_max => true,
                    _ => false,
                }
            }

            fn not_present(&self) -> Option<Self::Base> {
                if self.is_not_present() {
                    Some(self.0)
                } else {
                    None
                }
            }

            fn is_not_present(&self) -> bool {
                match self.0 {
                    $not_present_min..=$not_present_max => true,
                    _ => false,
                }
            }
        }

        impl From<$type> for $base {
            fn from(value: $type) -> Self {
                value.0
            }
        }

        impl From<$base> for $type {
            fn from(value: $base) -> Self {
                Self(value)
            }
        }
    };
}

// rustfmt::skip
signal_impl!(
    Param4,
    u8,
    valid:          0x00..=0x0A,
    indicator:      0x0B..=0x0B,
    error:          0x0E..=0x0E,
    not_present:    0x0F..=0x0F,
);
signal_impl!(
    Param8,
    u8,
    valid:          0x00..=0xFA,
    indicator:      0xFB..=0xFB,
    error:          0xFE..=0xFE,
    not_present:    0xFF..=0xFF,
);
signal_impl!(
    Param10,
    u16,
    valid:          0x000..=0x3FA,
    indicator:      0x3FB..=0x3FB,
    error:          0x3FE..=0x3FE,
    not_present:    0x3FF..=0x3FF,
);
signal_impl!(
    Param12,
    u16,
    valid:          0x000..=0xFAF,
    indicator:      0xFB0..=0xFBF,
    error:          0xFE0..=0xFEF,
    not_present:    0xFF0..=0xFFF,
);
signal_impl!(
    Param16,
    u16,
    valid:          0x0000..=0xFAFF,
    indicator:      0xFB00..=0xFBFF,
    error:          0xFE00..=0xFEFF,
    not_present:    0xFF00..=0xFFFF,
);
signal_impl!(
    Param20,
    u32,
    valid:          0x00000..=0xFAFFF,
    indicator:      0xFB000..=0xFBFFF,
    error:          0xFE000..=0xFEFFF,
    not_present:    0xFF000..=0xFFFFF,
);
signal_impl!(
    Param24,
    u32,
    valid:          0x000000..=0xFAFFFF,
    indicator:      0xFB0000..=0xFBFFFF,
    error:          0xFE0000..=0xFEFFFF,
    not_present:    0xFF0000..=0xFFFFFF,
);
signal_impl!(
    Param28,
    u32,
    valid:          0x0000000..=0xFAFFFFF,
    indicator:      0xFB00000..=0xFBFFFFF,
    error:          0xFE00000..=0xFEFFFFF,
    not_present:    0xFF00000..=0xFFFFFFF,
);
signal_impl!(
    Param32,
    u32,
    valid:          0x00000000..=0xFAFFFFFF,
    indicator:      0xFB000000..=0xFBFFFFFF,
    error:          0xFE000000..=0xFEFFFFFF,
    not_present:    0xFF000000..=0xFFFFFFFF,
);

/// Discrete parameter
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-1", derive(defmt::Format))]
pub enum Discrete {
    Disabled = 0b00,
    Enabled = 0b01,
    ErrorIndicator = 0b10,
    NotAvailable = 0b11,
}

impl TryFrom<u8> for Discrete {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            x if x == Self::Disabled as u8 => Ok(Self::Disabled),
            x if x == Self::Enabled as u8 => Ok(Self::Enabled),
            x if x == Self::ErrorIndicator as u8 => Ok(Self::ErrorIndicator),
            x if x == Self::NotAvailable as u8 => Ok(Self::NotAvailable),
            _ => Err(value),
        }
    }
}

impl From<Discrete> for u8 {
    fn from(value: Discrete) -> Self {
        value as u8
    }
}

/// Control command
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-1", derive(defmt::Format))]
pub enum Command {
    Disable = 0b00,
    Enable = 0b01,
    Reserved = 0b10,
    NoAction = 0b11,
}

impl TryFrom<u8> for Command {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            x if x == Self::Disable as u8 => Ok(Self::Disable),
            x if x == Self::Enable as u8 => Ok(Self::Enable),
            x if x == Self::Reserved as u8 => Ok(Self::Reserved),
            x if x == Self::NoAction as u8 => Ok(Self::NoAction),
            _ => Err(value),
        }
    }
}

impl From<Command> for u8 {
    fn from(value: Command) -> Self {
        value as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_range() {
        assert!(Param4(0x0).is_valid());
        assert!(Param8(0x0).is_valid());
        assert!(Param12(0x0).is_valid());
        assert!(Param16(0x0).is_valid());
        assert!(Param20(0x0).is_valid());
        assert!(Param24(0x0).is_valid());
        assert!(Param28(0x0).is_valid());
        assert!(Param32(0x0).is_valid());
    }

    #[test]
    fn outside_signal() {
        assert!(Param4::from_raw(0xF + 1).is_none());
        assert!(Param10::from_raw(0x3FF + 1).is_none());
        assert!(Param12::from_raw(0xFFF + 1).is_none());
        assert!(Param20::from_raw(0xFFFFF + 1).is_none());
        assert!(Param24::from_raw(0xFFFFFF + 1).is_none());
        assert!(Param28::from_raw(0xFFFFFFF + 1).is_none());
    }

    #[test]
    fn value() {
        assert_eq!(Param4::from_raw(0x0).unwrap().value(), Some(0x0));
        assert_eq!(Param4::from_raw(0xA).unwrap().value(), Some(0xA));
        assert_eq!(Param4::from_raw(0xF).unwrap().value(), None);
    }

    #[test]
    fn from_to_raw() {
        assert_eq!(Param4::from_raw(0x1).unwrap().to_raw(), 0x1);
        assert_eq!(Param8::from_raw(0x1).unwrap().to_raw(), 0x1);
        assert_eq!(Param10::from_raw(0x1).unwrap().to_raw(), 0x1);
        assert_eq!(Param12::from_raw(0x1).unwrap().to_raw(), 0x1);
        assert_eq!(Param16::from_raw(0x1).unwrap().to_raw(), 0x1);
        assert_eq!(Param20::from_raw(0x1).unwrap().to_raw(), 0x1);
        assert_eq!(Param24::from_raw(0x1).unwrap().to_raw(), 0x1);
        assert_eq!(Param28::from_raw(0x1).unwrap().to_raw(), 0x1);
        assert_eq!(Param32::from_raw(0x1).unwrap().to_raw(), 0x1);
    }
}
