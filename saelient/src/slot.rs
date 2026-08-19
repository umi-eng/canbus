//! Scaling, limit, offset, and transfer functions (J1939-73)

use crate::signal::Param8;
use crate::signal::Param16;
use crate::signal::Signal;
use num::FromPrimitive;
use num::cast::AsPrimitive;

/// Errors returned when converting between a slot and its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SlotError {
    #[error("value cannot be represented by the signal's base type")]
    FloatConversion,
    #[error("signal parameter does not contain a valid value")]
    InvalidParameter,
    #[error("invalid signal parameter: {0}")]
    Signal(#[from] crate::signal::SignalError),
}

pub trait Slot<T: Signal>: Sized {
    /// Unit of measurement.
    const UNIT: &str;
    /// Value offset.
    const OFFSET: f32 = 0.0;
    /// Value scale factor.
    const SCALE: f32;

    /// Create a new instance of this slot from the underlying parameter.
    fn new(parameter: T) -> Self;

    /// Get the underlying paramter from this slot.
    fn parameter(&self) -> T;

    /// Try converting from an f32.
    fn from_f32(value: f32) -> Result<Self, SlotError> {
        let value = (value - Self::OFFSET) / Self::SCALE;
        let value = if value >= 0.0 {
            value + 0.5
        } else {
            value - 0.5
        };
        let value = T::Base::from_f32(value).ok_or(SlotError::FloatConversion)?;
        let parameter = T::from_raw(value)?;
        Ok(Self::new(parameter))
    }

    /// Try converting to an f32.
    fn as_f32(&self) -> Result<f32, SlotError> {
        let parameter = self.parameter();
        let value: u32 = parameter.value().ok_or(SlotError::InvalidParameter)?.as_();
        let value = (value as f32 * Self::SCALE) + Self::OFFSET;
        Ok(value)
    }
}

#[macro_export]
macro_rules! slot_impl {
    ($type:ident, $param:ident, $offset:expr, $scale:expr, $unit:expr, $comment:expr) => {
        #[doc = $comment]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $type($param);

        impl Slot<$param> for $type {
            const UNIT: &str = $unit;
            const OFFSET: f32 = $offset;
            const SCALE: f32 = $scale;

            fn new(parameter: $param) -> Self {
                Self(parameter)
            }

            fn parameter(&self) -> $param {
                self.0
            }
        }
    };
}

slot_impl!(
    SaeTP01,
    Param8,
    -40.0,
    1.0,
    "°C",
    "Temperature - 1 °C per bit"
);
slot_impl!(
    SaeEC06,
    Param16,
    0.0,
    0.001,
    "A",
    "Current - 0.001 A per bit"
);
slot_impl!(SaeEC09, Param8, 0.0, 0.25, "A", "Current - 0.25 A per bit");
slot_impl!(
    SaeEV06,
    Param16,
    0.0,
    0.001,
    "V",
    "Voltage - 0.001 V per bit"
);
slot_impl!(SaePC03, Param8, 0.0, 0.004, "%", "Percent - 0.4% per bit");
slot_impl!(SaePC04, Param8, -1.0, 0.008, "%", "Percent - 0.8% per bit");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_sae_tp01() {
        let slot = SaeTP01::from_f32(210.0).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 250);
        assert_eq!(slot.as_f32(), Ok(210.0));

        let slot = SaeTP01::from_f32(-40.0).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 0);
        assert_eq!(slot.as_f32(), Ok(-40.0));

        let slot = SaeTP01::from_f32(0.0).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 40);
        assert_eq!(slot.as_f32(), Ok(0.0));
    }

    #[test]
    fn slot_sae_ec06() {
        let slot = SaeEC06::from_f32(0.0).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 0);
        assert_eq!(slot.as_f32(), Ok(0.0));

        // "rounded" to the nearest representable float
        let slot = SaeEC06::from_f32(24.000002).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 24000);
        assert_eq!(slot.as_f32(), Ok(24.000002));

        // "rounded" to the nearest representable float
        let slot = SaeEC06::from_f32(64.225006).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 64225);
        assert_eq!(slot.as_f32(), Ok(64.225006));
    }

    #[test]
    fn slot_sae_ec09() {
        let slot = SaeEC09::from_f32(0.0).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 0);
        assert_eq!(slot.as_f32(), Ok(0.0));

        let slot = SaeEC09::from_f32(31.25).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 125);
        assert_eq!(slot.as_f32(), Ok(31.25));

        let slot = SaeEC09::from_f32(62.5).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 250);
        assert_eq!(slot.as_f32(), Ok(62.5));
    }

    #[test]
    fn slot_sae_ev06() {
        let slot = SaeEV06::from_f32(0.0).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 0);
        assert_eq!(slot.as_f32(), Ok(0.0));

        // "rounded" to the nearest representable float
        let slot = SaeEV06::from_f32(24.000002).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 24000);
        assert_eq!(slot.as_f32(), Ok(24.000002));

        // "rounded" to the nearest representable float
        let slot = SaeEV06::from_f32(64.225006).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 64225);
        assert_eq!(slot.as_f32(), Ok(64.225006));
    }

    #[test]
    fn slot_sae_pc03() {
        let slot = SaePC03::from_f32(0.0).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 0);
        assert_eq!(slot.as_f32(), Ok(0.0));

        let slot = SaePC03::from_f32(0.30).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 75);
        assert_eq!(slot.as_f32(), Ok(0.30));

        let slot = SaePC03::from_f32(1.0).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 250);
        assert_eq!(slot.as_f32(), Ok(1.0));

        // Negative values produce a negative raw index, which is out of range
        assert!(SaePC03::from_f32(-0.004).is_err());
    }

    #[test]
    fn slot_sae_pc04() {
        let slot = SaePC04::from_f32(-1.0).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 0);
        assert_eq!(slot.as_f32(), Ok(-1.0));

        let slot = SaePC04::from_f32(0.0).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 125);
        assert_eq!(slot.as_f32(), Ok(0.0));

        let slot = SaePC04::from_f32(0.20).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 150);
        assert_eq!(slot.as_f32(), Ok(0.20000005)); // 150 * 0.008f32 - 1.0; scale not exact in f32

        let slot = SaePC04::from_f32(1.0).unwrap();
        assert_eq!(slot.parameter().value().unwrap(), 250);
        assert_eq!(slot.as_f32(), Ok(1.0));

        // Below offset produces a negative raw index, which is out of range
        assert!(SaePC04::from_f32(-1.008).is_err());
    }
}
