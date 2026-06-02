use crate::id::Pgn;

/// Request to send (TP.CM_RTS) message.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "defmt-1", derive(defmt::Format))]
pub struct RequestToSend {
    total_size: u16,
    total_packets: u8,
    max_packets_per_response: Option<u8>,
    pgn: Pgn,
}

impl RequestToSend {
    const MUX: u8 = 16;

    /// Create a new request to send message.
    ///
    /// - `total_size` must be between 9 and 1785 bytes.
    /// - `max_packets_per_response` must be between
    pub fn new(total_size: u16, max_packets_per_response: Option<u8>, pgn: Pgn) -> Self {
        assert!(total_size <= 1785);
        assert!(total_size >= 9);

        let total_packets = total_size.div_ceil(7);
        assert!(total_packets >= 2);
        assert!(total_packets <= 255);
        let total_packets = total_packets as u8;

        if let Some(max) = max_packets_per_response {
            assert!(
                max >= 1 && max < 255,
                "max_packets_per_response must be 1–254; use `None` for no limit (0x00 and 0xFF are not valid)"
            );
        }

        Self {
            total_size,
            total_packets,
            max_packets_per_response,
            pgn,
        }
    }

    /// Total number of bytes in this transfer.
    pub fn total_size(&self) -> u16 {
        self.total_size
    }

    /// Total number of packets in this transfer.
    pub fn total_packets(&self) -> u8 {
        self.total_packets
    }

    /// The maximum number of packets the sender is allowed to respond with for
    /// every TP.CM_CTS message.
    ///
    /// `None` signifies no limit.
    pub fn max_packets_per_response(&self) -> Option<u8> {
        self.max_packets_per_response
    }

    /// Tranfer contents PGN.
    pub fn pgn(&self) -> Pgn {
        self.pgn
    }
}

impl From<RequestToSend> for [u8; 8] {
    fn from(val: RequestToSend) -> Self {
        let total_size = val.total_size.to_le_bytes();
        let pgn = u32::from(val.pgn).to_le_bytes();
        [
            RequestToSend::MUX,
            total_size[0],
            total_size[1],
            val.total_packets,
            val.max_packets_per_response.unwrap_or(255),
            pgn[0],
            pgn[1],
            pgn[2],
        ]
    }
}

impl<'a> TryFrom<&'a [u8]> for RequestToSend {
    type Error = &'a [u8];

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        if value.len() != 8 {
            return Err(value);
        }

        if value[0] != Self::MUX {
            return Err(value);
        }

        Ok(Self {
            total_size: u16::from_le_bytes([value[1], value[2]]),
            total_packets: value[3],
            max_packets_per_response: match value[4] {
                0..255 => Some(value[4]),
                255 => None,
            },
            pgn: Pgn::from(u32::from_le_bytes([value[5], value[6], value[7], 0x00])),
        })
    }
}

/// Clear to send (TP.CM_CTS) message.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "defmt-1", derive(defmt::Format))]
pub struct ClearToSend {
    max_packets_per_response: Option<u8>,
    next_sequence: u8,
    pgn: Pgn,
}

impl ClearToSend {
    const MUX: u8 = 17;

    /// Create a new CTS message.
    pub fn new(max_packets_per_response: Option<u8>, next_sequence: u8, pgn: Pgn) -> Self {
        Self {
            max_packets_per_response,
            next_sequence,
            pgn,
        }
    }

    /// Number of packets that can be sent sent.
    pub fn max_packets_per_response(&self) -> Option<u8> {
        self.max_packets_per_response
    }

    /// Next sequence number.
    pub fn next_sequence(&self) -> u8 {
        self.next_sequence
    }
}

impl From<&ClearToSend> for [u8; 8] {
    fn from(value: &ClearToSend) -> Self {
        let pgn = u32::from(value.pgn).to_le_bytes();

        [
            ClearToSend::MUX,
            value.max_packets_per_response.unwrap_or(255),
            value.next_sequence,
            0xFF, // reserved
            0xFF, // reserved
            pgn[0],
            pgn[1],
            pgn[2],
        ]
    }
}

impl<'a> TryFrom<&'a [u8]> for ClearToSend {
    type Error = &'a [u8];

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        if value.len() != 8 {
            return Err(value);
        }

        if value[0] != Self::MUX {
            return Err(value);
        }

        let pgn = Pgn::from(u32::from_le_bytes([value[5], value[6], value[7], 0x00]));

        Ok(Self {
            max_packets_per_response: match value[1] {
                0..255 => Some(value[1]),
                255 => None,
            },
            next_sequence: value[2],
            pgn,
        })
    }
}

/// End of message acknowledge (TP.CM_EndOfMsgAck) message.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "defmt-1", derive(defmt::Format))]
pub struct EndOfMessageAck {
    total_size: u16,
    total_packets: u8,
    pgn: Pgn,
}

impl EndOfMessageAck {
    const MUX: u8 = 19;

    /// Creates a new end of message acknowledge message.
    pub fn new(total_size: u16, total_packets: u8, pgn: Pgn) -> Self {
        Self {
            total_size,
            total_packets,
            pgn,
        }
    }

    /// Total message size in bytes.
    pub fn total_size(&self) -> u16 {
        self.total_size
    }

    /// Total number of packets transferred.
    pub fn total_packets(&self) -> u8 {
        self.total_packets
    }

    /// Tranfer contents PGN.
    pub fn pgn(&self) -> Pgn {
        self.pgn
    }
}

impl From<&EndOfMessageAck> for [u8; 8] {
    fn from(value: &EndOfMessageAck) -> Self {
        let total_size = value.total_size.to_le_bytes();
        let pgn = u32::from(value.pgn).to_le_bytes();

        [
            EndOfMessageAck::MUX,
            total_size[0],
            total_size[1],
            value.total_packets,
            0xFF,
            pgn[0],
            pgn[1],
            pgn[2],
        ]
    }
}

impl<'a> TryFrom<&'a [u8]> for EndOfMessageAck {
    type Error = &'a [u8];

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        if value.len() != 8 {
            return Err(value);
        }

        if value[0] != Self::MUX {
            return Err(value);
        }

        let total_size = u16::from_le_bytes([value[1], value[2]]);

        let total_packets = value[3];

        let pgn = Pgn::from(u32::from_le_bytes([value[5], value[6], value[7], 0x00]));

        Ok(Self {
            total_size,
            total_packets,
            pgn,
        })
    }
}

/// Connection abort (TP.Conn_Abort) message.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "defmt-1", derive(defmt::Format))]
pub struct ConnectionAbort {
    reason: AbortReason,
    sender_role: AbortSenderRole,
    pgn: Pgn,
}

impl ConnectionAbort {
    const MUX: u8 = 255;

    /// Create a new connection abort message.
    pub fn new(reason: AbortReason, sender_role: AbortSenderRole, pgn: Pgn) -> Self {
        Self {
            reason,
            sender_role,
            pgn,
        }
    }

    /// Abort reason.
    pub fn reason(&self) -> AbortReason {
        self.reason
    }

    /// Abort sender role.
    pub fn sender_role(&self) -> AbortSenderRole {
        self.sender_role
    }

    /// Tranfer contents PGN.
    pub fn pgn(&self) -> Pgn {
        self.pgn
    }
}

impl<'a> TryFrom<&'a [u8]> for ConnectionAbort {
    type Error = &'a [u8];

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        if value.len() != 8 {
            return Err(value);
        }

        if value[0] != Self::MUX {
            return Err(value);
        }

        Ok(Self {
            reason: AbortReason::try_from(value[1]).unwrap_or(AbortReason::Custom),
            sender_role: AbortSenderRole::try_from(value[2] & 0b00000011)
                .unwrap_or(AbortSenderRole::NotSpecified),
            pgn: Pgn::from(u32::from_le_bytes([value[5], value[6], value[7], 0x00])),
        })
    }
}

impl From<&ConnectionAbort> for [u8; 8] {
    fn from(value: &ConnectionAbort) -> Self {
        let pgn = u32::from(value.pgn).to_le_bytes();

        [
            ConnectionAbort::MUX,
            u8::from(&value.reason),
            u8::from(&value.sender_role) | 0b11111100,
            0xFF,
            0xFF,
            pgn[0],
            pgn[1],
            pgn[2],
        ]
    }
}

/// Abort reason.
///
/// See J1939™-21 table 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-1", derive(defmt::Format))]
pub enum AbortReason {
    /// Already in one or more connection managed sessions and cannot support another.
    MaxConnections = 1,
    /// System resources were needed for another task, so this connection managed session was terminated.
    CanceledBySystem = 2,
    /// A timeout occurred, and this is the connection abort to close the session.
    Timeout = 3,
    /// CTS messages received when data transfer is in progress.
    CtsWhileDataTransfer = 4,
    /// Maximum retransmit request limit reached.
    RetransmitLimitReached = 5,
    /// Unexpected data transfer packet.
    UnexpectedDataTransfer = 6,
    /// Bad sequence number (software cannot recover).
    BadSequenceNumber = 7,
    /// Duplicate sequence number (software cannot recover).
    DuplicateSequenceNumber = 8,
    /// Total Message Size is greater than 1785 bytes.
    MessageTooLarge = 9,
    /// If a Connection Abort reason is identified that is not listed in the table use code 250.
    Custom = 250,
}

impl TryFrom<u8> for AbortReason {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            x if x == Self::MaxConnections as u8 => Ok(Self::MaxConnections),
            x if x == Self::CanceledBySystem as u8 => Ok(Self::CanceledBySystem),
            x if x == Self::Timeout as u8 => Ok(Self::Timeout),
            x if x == Self::CtsWhileDataTransfer as u8 => Ok(Self::CtsWhileDataTransfer),
            x if x == Self::RetransmitLimitReached as u8 => Ok(Self::RetransmitLimitReached),
            x if x == Self::UnexpectedDataTransfer as u8 => Ok(Self::UnexpectedDataTransfer),
            x if x == Self::BadSequenceNumber as u8 => Ok(Self::BadSequenceNumber),
            x if x == Self::DuplicateSequenceNumber as u8 => Ok(Self::DuplicateSequenceNumber),
            x if x == Self::MessageTooLarge as u8 => Ok(Self::MessageTooLarge),
            x if x == Self::Custom as u8 => Ok(Self::Custom),
            _ => Err(value),
        }
    }
}

impl From<&AbortReason> for u8 {
    fn from(value: &AbortReason) -> Self {
        *value as u8
    }
}

/// Abort message sender role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-1", derive(defmt::Format))]
pub enum AbortSenderRole {
    Sender = 0b00,
    Receiver = 0b01,
    Reserved = 0b10,
    NotSpecified = 0b11,
}

impl TryFrom<u8> for AbortSenderRole {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            x if x == Self::Sender as u8 => Ok(Self::Sender),
            x if x == Self::Receiver as u8 => Ok(Self::Receiver),
            x if x == Self::Reserved as u8 => Ok(Self::Reserved),
            x if x == Self::NotSpecified as u8 => Ok(Self::NotSpecified),
            _ => Err(value),
        }
    }
}

impl From<&AbortSenderRole> for u8 {
    fn from(value: &AbortSenderRole) -> Self {
        *value as u8
    }
}

/// Data transfer (TP.DT) message.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "defmt-1", derive(defmt::Format))]
pub struct DataTransfer {
    sequence: u8,
    data: [u8; 7],
}

impl DataTransfer {
    /// Create a new data transfer message.
    ///
    /// The sequence number starts at 1 and continues up to the maximum of 255.
    ///
    /// Data with less than 7 bytes should have the remaining bytes padded with
    /// 0xFF.
    pub fn new(sequence: u8, data: [u8; 7]) -> Self {
        Self { sequence, data }
    }

    /// Packet sequence number.
    pub fn sequence(&self) -> u8 {
        self.sequence
    }

    /// Payload data.
    pub fn data(&self) -> [u8; 7] {
        self.data
    }
}

impl From<&DataTransfer> for [u8; 8] {
    fn from(value: &DataTransfer) -> Self {
        [
            value.sequence,
            value.data[0],
            value.data[1],
            value.data[2],
            value.data[3],
            value.data[4],
            value.data[5],
            value.data[6],
        ]
    }
}

impl<'a> TryFrom<&'a [u8]> for DataTransfer {
    type Error = &'a [u8];

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        if value.len() != 8 {
            return Err(value);
        }

        Ok(Self {
            sequence: value[0],
            data: [
                value[1], value[2], value[3], value[4], value[5], value[6], value[7],
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::Pgn;

    // J1939-21 5.10.2 - TP.CM_RTS (Control Byte = 16)

    #[test]
    fn rts_construct_and_getters() {
        let pgn = Pgn::TransportProtocolConnectionManagement;
        let rts = RequestToSend::new(100, Some(5), pgn);
        assert_eq!(rts.total_size(), 100);
        // 100 bytes -> ceil(100/7) = 15 packets
        assert_eq!(rts.total_packets(), 15);
        assert_eq!(rts.max_packets_per_response(), Some(5));
        assert_eq!(rts.pgn(), pgn);
    }

    #[test]
    fn rts_no_limit_serialises_as_ff() {
        let pgn = Pgn::MemoryAccessRequest;
        let rts = RequestToSend::new(14, None, pgn);
        let bytes: [u8; 8] = rts.into();
        // byte 0: control byte 16 (0x10)
        assert_eq!(bytes[0], 16);
        // byte 4: 0xFF means no limit
        assert_eq!(bytes[4], 0xFF);
    }

    #[test]
    fn rts_serialise_layout() {
        // J1939-21 table 4: [CB, size_lo, size_hi, total_pkts, max_pkts, pgn_lo, pgn_mid, pgn_hi]
        let pgn = Pgn::Request; // u32 = 59904 = 0x00EA00
        let rts = RequestToSend::new(9, Some(1), pgn);
        let bytes: [u8; 8] = rts.into();
        assert_eq!(bytes[0], 16); // control byte
        assert_eq!(bytes[1], 9); // size low byte
        assert_eq!(bytes[2], 0); // size high byte
        assert_eq!(bytes[3], 2); // ceil(9/7) = 2 packets
        assert_eq!(bytes[4], 1); // max_packets_per_response
        assert_eq!(
            u32::from_le_bytes([bytes[5], bytes[6], bytes[7], 0]),
            u32::from(pgn)
        );
    }

    #[test]
    fn rts_try_from_roundtrip() {
        let pgn = Pgn::MemoryAccessResponse;
        let rts = RequestToSend::new(21, Some(3), pgn);
        let bytes: [u8; 8] = rts.clone().into();
        let decoded = RequestToSend::try_from(bytes.as_ref()).unwrap();
        assert_eq!(decoded.total_size(), rts.total_size());
        assert_eq!(decoded.total_packets(), rts.total_packets());
        assert_eq!(
            decoded.max_packets_per_response(),
            rts.max_packets_per_response()
        );
        assert_eq!(decoded.pgn(), rts.pgn());
    }

    #[test]
    fn rts_try_from_no_limit_roundtrip() {
        let pgn = Pgn::Request;
        let rts = RequestToSend::new(14, None, pgn);
        let bytes: [u8; 8] = rts.into();
        let decoded = RequestToSend::try_from(bytes.as_ref()).unwrap();
        assert_eq!(decoded.max_packets_per_response(), None);
    }

    #[test]
    fn rts_try_from_wrong_mux() {
        let raw: &[u8] = &[17, 0x09, 0x00, 0x02, 0x01, 0x00, 0xEA, 0x00];
        assert!(RequestToSend::try_from(raw).is_err());
    }

    #[test]
    fn rts_try_from_wrong_length() {
        let raw: &[u8] = &[16, 0x09, 0x00];
        assert!(RequestToSend::try_from(raw).is_err());
    }

    #[test]
    #[should_panic]
    fn rts_total_size_below_minimum_panics() {
        RequestToSend::new(8, None, Pgn::Request);
    }

    #[test]
    #[should_panic]
    fn rts_total_size_above_maximum_panics() {
        RequestToSend::new(1786, None, Pgn::Request);
    }

    #[test]
    #[should_panic]
    fn rts_max_packets_255_panics() {
        // 255 is reserved for "no limit" - must use `None`
        RequestToSend::new(14, Some(255), Pgn::Request);
    }

    #[test]
    fn rts_max_total_size_254_packets() {
        // 1785 bytes -> ceil(1785/7) = 255 packets
        let rts = RequestToSend::new(1785, None, Pgn::Request);
        assert_eq!(rts.total_packets(), 255);
    }

    // J1939-21 5.10.3 - TP.CM_CTS (Control Byte = 17)

    #[test]
    fn cts_construct_and_getters() {
        let pgn = Pgn::Request;
        let cts = ClearToSend::new(Some(5), 3, pgn);
        assert_eq!(cts.max_packets_per_response(), Some(5));
        assert_eq!(cts.next_sequence(), 3);
    }

    #[test]
    fn cts_serialise_layout() {
        // J1939-21 table 5: [CB=17, max_pkts, next_seq, FF, FF, pgn_lo, pgn_mid, pgn_hi]
        let pgn = Pgn::Request;
        let cts = ClearToSend::new(Some(3), 1, pgn);
        let bytes: [u8; 8] = (&cts).into();
        assert_eq!(bytes[0], 17);
        assert_eq!(bytes[1], 3);
        assert_eq!(bytes[2], 1);
        assert_eq!(bytes[3], 0xFF); // reserved
        assert_eq!(bytes[4], 0xFF); // reserved
        assert_eq!(
            u32::from_le_bytes([bytes[5], bytes[6], bytes[7], 0]),
            u32::from(pgn)
        );
    }

    #[test]
    fn cts_no_limit_serialises_as_ff() {
        let cts = ClearToSend::new(None, 1, Pgn::Request);
        let bytes: [u8; 8] = (&cts).into();
        assert_eq!(bytes[1], 0xFF);
    }

    #[test]
    fn cts_try_from_roundtrip() {
        let pgn = Pgn::MemoryAccessRequest;
        let cts = ClearToSend::new(Some(7), 4, pgn);
        let bytes: [u8; 8] = (&cts).into();
        let decoded = ClearToSend::try_from(bytes.as_ref()).unwrap();
        assert_eq!(
            decoded.max_packets_per_response(),
            cts.max_packets_per_response()
        );
        assert_eq!(decoded.next_sequence(), cts.next_sequence());
    }

    #[test]
    fn cts_try_from_no_limit_roundtrip() {
        let cts = ClearToSend::new(None, 1, Pgn::Request);
        let bytes: [u8; 8] = (&cts).into();
        let decoded = ClearToSend::try_from(bytes.as_ref()).unwrap();
        assert_eq!(decoded.max_packets_per_response(), None);
    }

    #[test]
    fn cts_try_from_wrong_mux() {
        let raw: &[u8] = &[16, 5, 1, 0xFF, 0xFF, 0x00, 0xEA, 0x00];
        assert!(ClearToSend::try_from(raw).is_err());
    }

    #[test]
    fn cts_try_from_wrong_length() {
        let raw: &[u8] = &[17, 5];
        assert!(ClearToSend::try_from(raw).is_err());
    }

    // J1939-21 5.10.5 - TP.CM_EndOfMsgAck (Control Byte = 19)

    #[test]
    fn eom_ack_construct_and_getters() {
        let pgn = Pgn::Request;
        let ack = EndOfMessageAck::new(100, 15, pgn);
        assert_eq!(ack.total_size(), 100);
        assert_eq!(ack.total_packets(), 15);
        assert_eq!(ack.pgn(), pgn);
    }

    #[test]
    fn eom_ack_serialise_layout() {
        // J1939-21 table 7: [CB=19, size_lo, size_hi, total_pkts, FF, pgn_lo, pgn_mid, pgn_hi]
        let pgn = Pgn::Request; // 0x00EA00
        let ack = EndOfMessageAck::new(200, 29, pgn);
        let bytes: [u8; 8] = (&ack).into();
        assert_eq!(bytes[0], 19);
        assert_eq!(u16::from_le_bytes([bytes[1], bytes[2]]), 200);
        assert_eq!(bytes[3], 29);
        assert_eq!(bytes[4], 0xFF); // reserved
        assert_eq!(
            u32::from_le_bytes([bytes[5], bytes[6], bytes[7], 0]),
            u32::from(pgn)
        );
    }

    #[test]
    fn eom_ack_try_from_roundtrip() {
        let pgn = Pgn::MemoryAccessResponse;
        let ack = EndOfMessageAck::new(1785, 255, pgn);
        let bytes: [u8; 8] = (&ack).into();
        let decoded = EndOfMessageAck::try_from(bytes.as_ref()).unwrap();
        assert_eq!(decoded.total_size(), ack.total_size());
        assert_eq!(decoded.total_packets(), ack.total_packets());
        assert_eq!(decoded.pgn(), ack.pgn());
    }

    #[test]
    fn eom_ack_try_from_wrong_mux() {
        let raw: &[u8] = &[16, 0xC8, 0x00, 0x1D, 0xFF, 0x00, 0xEA, 0x00];
        assert!(EndOfMessageAck::try_from(raw).is_err());
    }

    #[test]
    fn eom_ack_try_from_wrong_length() {
        let raw: &[u8] = &[19, 0xC8];
        assert!(EndOfMessageAck::try_from(raw).is_err());
    }

    // J1939-21 5.10.4 - TP.Conn_Abort (Control Byte = 255)

    #[test]
    fn connection_abort_construct_and_getters() {
        let pgn = Pgn::Request;
        let abort = ConnectionAbort::new(AbortReason::Timeout, AbortSenderRole::Sender, pgn);
        assert_eq!(abort.reason(), AbortReason::Timeout);
        assert_eq!(abort.sender_role(), AbortSenderRole::Sender);
        assert_eq!(abort.pgn(), pgn);
    }

    #[test]
    fn connection_abort_serialise_layout() {
        // J1939-21 table 6: [CB=255, reason, role|0b11111100, FF, FF, pgn_lo, pgn_mid, pgn_hi]
        let pgn = Pgn::Request;
        let abort = ConnectionAbort::new(AbortReason::Timeout, AbortSenderRole::Receiver, pgn);
        let bytes: [u8; 8] = (&abort).into();
        assert_eq!(bytes[0], 255);
        assert_eq!(bytes[1], AbortReason::Timeout as u8);
        // role bits are in bits [1:0]; upper 6 bits must be set to 1 per J1939-21
        assert_eq!(bytes[2] & 0b11, AbortSenderRole::Receiver as u8);
        assert_eq!(bytes[2] & 0b11111100, 0b11111100);
        assert_eq!(bytes[3], 0xFF);
        assert_eq!(bytes[4], 0xFF);
        assert_eq!(
            u32::from_le_bytes([bytes[5], bytes[6], bytes[7], 0]),
            u32::from(pgn)
        );
    }

    #[test]
    fn connection_abort_try_from_roundtrip_all_reasons() {
        let reasons = [
            AbortReason::MaxConnections,
            AbortReason::CanceledBySystem,
            AbortReason::Timeout,
            AbortReason::CtsWhileDataTransfer,
            AbortReason::RetransmitLimitReached,
            AbortReason::UnexpectedDataTransfer,
            AbortReason::BadSequenceNumber,
            AbortReason::DuplicateSequenceNumber,
            AbortReason::MessageTooLarge,
            AbortReason::Custom,
        ];
        for reason in reasons {
            let abort = ConnectionAbort::new(reason, AbortSenderRole::Sender, Pgn::Request);
            let bytes: [u8; 8] = (&abort).into();
            let decoded = ConnectionAbort::try_from(bytes.as_ref()).unwrap();
            assert_eq!(decoded.reason(), reason, "roundtrip failed for {reason:?}");
        }
    }

    #[test]
    fn connection_abort_unknown_reason_falls_back_to_custom() {
        // Byte 1 = 42 is not a defined abort reason; should fall back to Custom
        let raw: &[u8] = &[255, 42, 0b11111111, 0xFF, 0xFF, 0x00, 0xEA, 0x00];
        let decoded = ConnectionAbort::try_from(raw).unwrap();
        assert_eq!(decoded.reason(), AbortReason::Custom);
    }

    #[test]
    fn connection_abort_try_from_all_sender_roles() {
        let roles = [
            AbortSenderRole::Sender,
            AbortSenderRole::Receiver,
            AbortSenderRole::Reserved,
            AbortSenderRole::NotSpecified,
        ];
        for role in roles {
            let abort = ConnectionAbort::new(AbortReason::Timeout, role, Pgn::Request);
            let bytes: [u8; 8] = (&abort).into();
            let decoded = ConnectionAbort::try_from(bytes.as_ref()).unwrap();
            assert_eq!(decoded.sender_role(), role, "roundtrip failed for {role:?}");
        }
    }

    #[test]
    fn connection_abort_reserved_role_roundtrips_as_reserved() {
        // role bits = 0b10 (Reserved) — must decode back to Reserved, not NotSpecified.
        let raw: &[u8] = &[255, 3, 0b11111110, 0xFF, 0xFF, 0x00, 0xEA, 0x00];
        let decoded = ConnectionAbort::try_from(raw).unwrap();
        assert_eq!(decoded.sender_role(), AbortSenderRole::Reserved);
    }

    #[test]
    fn connection_abort_try_from_wrong_mux() {
        let raw: &[u8] = &[16, 3, 0xFF, 0xFF, 0xFF, 0x00, 0xEA, 0x00];
        assert!(ConnectionAbort::try_from(raw).is_err());
    }

    #[test]
    fn connection_abort_try_from_wrong_length() {
        let raw: &[u8] = &[255, 3];
        assert!(ConnectionAbort::try_from(raw).is_err());
    }

    // J1939-21 5.10.6 - TP.DT (sequence byte + 7 data bytes)

    #[test]
    fn data_transfer_construct_and_getters() {
        let data = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];
        let dt = DataTransfer::new(1, data);
        assert_eq!(dt.sequence(), 1);
        assert_eq!(dt.data(), data);
    }

    #[test]
    fn data_transfer_serialise_layout() {
        // J1939-21: byte 0 = sequence number, bytes 1-7 = data
        let data = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11];
        let dt = DataTransfer::new(3, data);
        let bytes: [u8; 8] = (&dt).into();
        assert_eq!(bytes[0], 3);
        assert_eq!(&bytes[1..], &data);
    }

    #[test]
    fn data_transfer_padded_packet() {
        // J1939-21: last packet should pad unused bytes with 0xFF
        let data = [0x01, 0x02, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let dt = DataTransfer::new(2, data);
        let bytes: [u8; 8] = (&dt).into();
        assert_eq!(bytes[3], 0xFF);
        assert_eq!(bytes[7], 0xFF);
    }

    #[test]
    fn data_transfer_try_from_roundtrip() {
        let data = [1, 2, 3, 4, 5, 6, 7];
        let dt = DataTransfer::new(5, data);
        let bytes: [u8; 8] = (&dt).into();
        let decoded = DataTransfer::try_from(bytes.as_ref()).unwrap();
        assert_eq!(decoded.sequence(), dt.sequence());
        assert_eq!(decoded.data(), dt.data());
    }

    #[test]
    fn data_transfer_try_from_wrong_length() {
        let raw: &[u8] = &[1, 2, 3];
        assert!(DataTransfer::try_from(raw).is_err());
        assert_eq!(DataTransfer::try_from(raw).unwrap_err(), raw);
    }

    #[test]
    fn abort_reason_try_from_all_valid() {
        let pairs: &[(u8, AbortReason)] = &[
            (1, AbortReason::MaxConnections),
            (2, AbortReason::CanceledBySystem),
            (3, AbortReason::Timeout),
            (4, AbortReason::CtsWhileDataTransfer),
            (5, AbortReason::RetransmitLimitReached),
            (6, AbortReason::UnexpectedDataTransfer),
            (7, AbortReason::BadSequenceNumber),
            (8, AbortReason::DuplicateSequenceNumber),
            (9, AbortReason::MessageTooLarge),
            (250, AbortReason::Custom),
        ];
        for (byte, expected) in pairs {
            assert_eq!(AbortReason::try_from(*byte).unwrap(), *expected);
            assert_eq!(u8::from(expected), *byte);
        }
    }

    #[test]
    fn abort_reason_try_from_invalid() {
        assert_eq!(AbortReason::try_from(42u8), Err(42));
        assert_eq!(AbortReason::try_from(0u8), Err(0));
    }

    #[test]
    fn abort_sender_role_try_from_all_valid() {
        assert_eq!(
            AbortSenderRole::try_from(0u8).unwrap(),
            AbortSenderRole::Sender
        );
        assert_eq!(
            AbortSenderRole::try_from(1u8).unwrap(),
            AbortSenderRole::Receiver
        );
        assert_eq!(
            AbortSenderRole::try_from(2u8).unwrap(),
            AbortSenderRole::Reserved
        );
        assert_eq!(
            AbortSenderRole::try_from(3u8).unwrap(),
            AbortSenderRole::NotSpecified
        );
    }

    #[test]
    fn abort_sender_role_try_from_invalid() {
        // 0b10 = Reserved, has no TryFrom mapping
        assert_eq!(AbortSenderRole::try_from(2u8), Err(2));
    }

    #[test]
    fn abort_sender_role_into_u8() {
        assert_eq!(u8::from(&AbortSenderRole::Sender), 0);
        assert_eq!(u8::from(&AbortSenderRole::Receiver), 1);
        assert_eq!(u8::from(&AbortSenderRole::Reserved), 2);
        assert_eq!(u8::from(&AbortSenderRole::NotSpecified), 3);
    }
}
