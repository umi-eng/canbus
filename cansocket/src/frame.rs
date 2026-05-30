use embedded_can::ExtendedId;
use embedded_can::Id;
use embedded_can::StandardId;

/// Frame creation error.
#[derive(Debug)]
pub enum FrameCreateError {
    InvalidDataLength,
}

impl std::error::Error for FrameCreateError {}

impl std::fmt::Display for FrameCreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDataLength => write!(f, "Invalid data length"),
        }
    }
}

/// CAN Bus frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame(pub(crate) libc::can_frame);

impl Frame {
    /// Create a new data frame.
    pub fn new(id: impl Into<Id>, data: &[u8]) -> Result<Self, FrameCreateError> {
        if data.len() > 8 {
            return Err(FrameCreateError::InvalidDataLength);
        }

        let mut frame: libc::can_frame = unsafe { std::mem::zeroed() };
        frame.can_id = match id.into() {
            Id::Extended(id) => id.as_raw() | libc::CAN_EFF_FLAG,
            Id::Standard(id) => id.as_raw().into(),
        };
        frame.data = data_from_slice(data)?;
        frame.can_dlc = data.len() as u8;
        Ok(Self(frame))
    }

    /// Create a new remote frame.
    pub fn new_remote(id: impl Into<Id>, len: u8) -> Result<Self, FrameCreateError> {
        if len > 8 {
            return Err(FrameCreateError::InvalidDataLength);
        }

        let mut frame: libc::can_frame = unsafe { std::mem::zeroed() };
        frame.can_id = match id.into() {
            Id::Extended(id) => id.as_raw() | libc::CAN_EFF_FLAG,
            Id::Standard(id) => id.as_raw().into(),
        } | libc::CAN_RTR_FLAG;
        frame.can_dlc = len;
        Ok(Self(frame))
    }
}

impl embedded_can::Frame for Frame {
    fn new(id: impl Into<Id>, data: &[u8]) -> Option<Self> {
        Frame::new(id, data).ok()
    }

    fn new_remote(id: impl Into<Id>, dlc: usize) -> Option<Self> {
        Frame::new_remote(id, dlc as u8).ok()
    }

    fn id(&self) -> Id {
        if self.is_extended() {
            Id::Extended(
                ExtendedId::new(self.0.can_id & libc::CAN_EFF_MASK).expect("extended id masking"),
            )
        } else {
            Id::Standard(
                StandardId::new((self.0.can_id & libc::CAN_SFF_MASK) as u16)
                    .expect("standard id masking"),
            )
        }
    }

    fn is_extended(&self) -> bool {
        (self.0.can_id & libc::CAN_EFF_FLAG) != 0
    }

    fn is_remote_frame(&self) -> bool {
        (self.0.can_id & libc::CAN_RTR_FLAG) != 0
    }

    fn dlc(&self) -> usize {
        self.0.can_dlc as usize
    }

    fn data(&self) -> &[u8] {
        &self.0.data[..self.0.can_dlc as usize]
    }
}

impl From<libc::can_frame> for Frame {
    fn from(value: libc::can_frame) -> Self {
        Self(value)
    }
}

impl From<Frame> for libc::can_frame {
    fn from(value: Frame) -> Self {
        value.0
    }
}

fn data_from_slice(data: &[u8]) -> Result<[u8; 8], FrameCreateError> {
    if data.len() > 8 {
        return Err(FrameCreateError::InvalidDataLength);
    }

    let mut bytes = [0; 8];
    bytes[..data.len()].copy_from_slice(data);
    Ok(bytes)
}
