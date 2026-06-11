/// A classical CAN 2.0 frame (max 8 data bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// CAN identifier (standard 11-bit or extended 29-bit).
    pub id: embedded_can::Id,
    /// Data bytes (meaningful up to `dlc`).
    pub data: [u8; 8],
    /// Data length code (0–8).
    pub dlc: u8,
    /// Remote transmission request flag.
    pub rtr: bool,
}

impl embedded_can::Frame for Frame {
    fn new(id: impl Into<embedded_can::Id>, data: &[u8]) -> Option<Self> {
        if data.len() > 8 {
            return None;
        }
        let mut buf = [0u8; 8];
        buf[..data.len()].copy_from_slice(data);
        Some(Self {
            id: id.into(),
            data: buf,
            dlc: data.len() as u8,
            rtr: false,
        })
    }

    fn new_remote(id: impl Into<embedded_can::Id>, dlc: usize) -> Option<Self> {
        if dlc > 8 {
            return None;
        }
        Some(Self {
            id: id.into(),
            data: [0u8; 8],
            dlc: dlc as u8,
            rtr: true,
        })
    }

    fn is_extended(&self) -> bool {
        matches!(self.id, embedded_can::Id::Extended(_))
    }

    fn is_remote_frame(&self) -> bool {
        self.rtr
    }

    fn id(&self) -> embedded_can::Id {
        self.id
    }

    fn dlc(&self) -> usize {
        self.dlc as usize
    }

    fn data(&self) -> &[u8] {
        if self.rtr {
            &[]
        } else {
            &self.data[..self.dlc as usize]
        }
    }
}

/// A CAN-FD frame (max 64 data bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FdFrame {
    /// CAN identifier (standard 11-bit or extended 29-bit).
    pub id: embedded_can::Id,
    /// Data bytes (meaningful up to `len`).
    pub data: [u8; 64],
    /// Actual data length in bytes (0–64, must be a valid FD DLC length).
    pub len: u8,
    /// Bit-rate switch — data phase transmitted at `data_bitrate`.
    pub brs: bool,
    /// Error state indicator — set by transmitter in error-passive state.
    pub esi: bool,
}

impl FdFrame {
    /// Create a new CAN-FD frame.
    pub fn new(id: impl Into<embedded_can::Id>, data: &[u8]) -> Option<Self> {
        if data.len() > 64 {
            return None;
        }
        let mut buf = [0u8; 64];
        buf[..data.len()].copy_from_slice(data);
        Some(Self {
            id: id.into(),
            data: buf,
            len: data.len() as u8,
            brs: false,
            esi: false,
        })
    }

    /// Create a new CAN-FD frame with BRS (bit-rate switch) enabled.
    pub fn new_brs(id: impl Into<embedded_can::Id>, data: &[u8]) -> Option<Self> {
        let mut f = Self::new(id, data)?;
        f.brs = true;
        Some(f)
    }
}

/// Convert a CAN-FD DLC value (0-15) to a data byte count.
pub fn fd_dlc_to_len(dlc: u8) -> usize {
    match dlc {
        0..=8 => dlc as usize,
        9 => 12,
        10 => 16,
        11 => 20,
        12 => 24,
        13 => 32,
        14 => 48,
        _ => 64,
    }
}

/// Convert a data byte count to the smallest valid CAN-FD DLC.
pub fn fd_len_to_dlc(len: usize) -> u8 {
    match len {
        0..=8 => len as u8,
        9..=12 => 9,
        13..=16 => 10,
        17..=20 => 11,
        21..=24 => 12,
        25..=32 => 13,
        33..=48 => 14,
        _ => 15,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_can::Frame as _;

    #[test]
    fn frame_std() {
        let f = Frame::new(embedded_can::StandardId::new(0x123).unwrap(), &[1, 2, 3]).unwrap();
        assert!(!f.is_extended());
        assert_eq!(f.dlc(), 3);
        assert_eq!(f.data(), &[1, 2, 3]);
    }

    #[test]
    fn frame_ext() {
        let f = Frame::new(embedded_can::ExtendedId::new(0x1234_ABCD).unwrap(), &[0xDE]).unwrap();
        assert!(f.is_extended());
        assert_eq!(f.dlc(), 1);
    }

    #[test]
    fn frame_rtr() {
        let f = Frame::new_remote(embedded_can::StandardId::new(0x42).unwrap(), 4).unwrap();
        assert!(f.is_remote_frame());
        assert_eq!(f.data(), &[] as &[u8]);
    }

    #[test]
    fn frame_too_long() {
        assert!(Frame::new(embedded_can::StandardId::new(0).unwrap(), &[0u8; 9]).is_none());
    }

    #[test]
    fn fd_frame_64() {
        let data = [0xABu8; 64];
        let f = FdFrame::new(embedded_can::StandardId::new(0x1).unwrap(), &data).unwrap();
        assert_eq!(f.len, 64);
        assert_eq!(&f.data[..], &data[..]);
    }

    #[test]
    fn fd_frame_too_long() {
        assert!(FdFrame::new(embedded_can::StandardId::new(0).unwrap(), &[0u8; 65]).is_none());
    }

    #[test]
    fn fd_dlc_all_lengths() {
        let cases = [
            (0, 0),
            (1, 1),
            (2, 2),
            (8, 8),
            (9, 12),
            (10, 16),
            (11, 20),
            (12, 24),
            (13, 32),
            (14, 48),
            (15, 64),
        ];
        for (dlc, expected_len) in cases {
            assert_eq!(fd_dlc_to_len(dlc), expected_len, "dlc={dlc}");
        }
    }

    #[test]
    fn fd_dlc_roundtrip() {
        for &len in &[0usize, 1, 4, 8, 12, 16, 20, 24, 32, 48, 64] {
            assert_eq!(fd_dlc_to_len(fd_len_to_dlc(len)), len);
        }
    }
}
