//! Transport protocol (J1939-21)

mod message;

use managed::ManagedSlice;
pub use message::AbortReason;
pub use message::AbortSenderRole;
pub use message::ClearToSend;
pub use message::ConnectionAbort;
pub use message::DataTransfer;
pub use message::EndOfMessageAck;
pub use message::RequestToSend;

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    StorageTooSmall,
    Sequence,
    PreviousAbort,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Response {
    Cts(ClearToSend),
    End(EndOfMessageAck),
}

impl From<&Response> for [u8; 8] {
    fn from(value: &Response) -> Self {
        match value {
            Response::Cts(cts) => cts.into(),
            Response::End(end) => end.into(),
        }
    }
}

/// An ongoing transport-protocol transfer.
#[derive(Debug)]
pub struct Transfer<'a> {
    rts: RequestToSend,
    rx_packets: u8,
    storage: ManagedSlice<'a, u8>,
    abort: bool,
}

impl<'a> Transfer<'a> {
    /// Create a new transfer from a RTS message received from the sender.
    #[cfg(feature = "alloc")]
    pub fn new(rts: RequestToSend) -> Self {
        Self {
            rts,
            rx_packets: 0,
            storage: Vec::new().into(),
            abort: false,
        }
    }

    /// Create a new transfer from a RTS message received from the sender using provided storage.
    pub fn new_with_storage(rts: RequestToSend, storage: impl Into<ManagedSlice<'a, u8>>) -> Self {
        Self {
            rts,
            rx_packets: 0,
            storage: storage.into(),
            abort: false,
        }
    }

    /// Return read-only acess to the internal buffer.
    ///
    /// The contents of this buffer are only valid after the transfer is complete.
    pub fn finished(&self) -> Option<&[u8]> {
        if self.rx_packets >= self.rts.total_packets() && !self.abort {
            Some(&self.storage[..self.rts.total_size() as usize])
        } else {
            None
        }
    }

    /// Feed the transfer with the next data transfer.
    pub fn next(
        &mut self,
        msg: DataTransfer,
    ) -> Result<Option<Response>, (Error, ConnectionAbort)> {
        if self.abort {
            return Err((
                Error::PreviousAbort,
                ConnectionAbort::new(
                    AbortReason::UnexpectedDataTransfer,
                    AbortSenderRole::Receiver,
                    self.rts.pgn(),
                ),
            ));
        }

        if msg.sequence() != self.rx_packets + 1 {
            self.abort = true;
            return Err((
                Error::Sequence,
                ConnectionAbort::new(
                    AbortReason::BadSequenceNumber,
                    AbortSenderRole::Receiver,
                    self.rts.pgn(),
                ),
            ));
        }

        match &mut self.storage {
            #[cfg(feature = "alloc")]
            ManagedSlice::Owned(vec) => {
                vec.extend_from_slice(&msg.data());
                vec.truncate(self.rts.total_size() as usize);
            }
            ManagedSlice::Borrowed(slice) => {
                let Some(chunk) = slice.chunks_mut(7).nth(self.rx_packets as usize) else {
                    self.abort = true;
                    return Err((
                        Error::StorageTooSmall,
                        ConnectionAbort::new(
                            AbortReason::Custom,
                            AbortSenderRole::Receiver,
                            self.rts.pgn(),
                        ),
                    ));
                };
                chunk.clone_from_slice(&msg.data()[..chunk.len()]);
            }
        }

        self.rx_packets += 1;

        if self.rx_packets == self.rts.total_packets() {
            return Ok(Some(Response::End(EndOfMessageAck::new(
                self.rts.total_size(),
                self.rts.total_packets(),
                self.rts.pgn(),
            ))));
        }

        if let Some(packets_per_response) = self.rts.max_packets_per_response() {
            // send cts on nth data transfer
            if msg.sequence().is_multiple_of(packets_per_response) {
                return Ok(Some(Response::Cts(ClearToSend::new(
                    self.rts.max_packets_per_response(),
                    self.rx_packets + 1,
                    self.rts.pgn(),
                ))));
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::Pgn;

    #[test]
    #[cfg(feature = "alloc")]
    fn transmission() {
        let rts = message::RequestToSend::new(16, Some(2), Pgn::ProprietaryA);
        let mut transfer = Transfer::new(rts);

        // send first data transfer
        let dt = message::DataTransfer::try_from([1, 1, 2, 3, 4, 5, 6, 7].as_ref()).unwrap();
        transfer.next(dt).unwrap();

        // send second data transfer which should trigger a CTS response.
        let dt = message::DataTransfer::try_from([2, 1, 2, 3, 4, 5, 6, 7].as_ref()).unwrap();
        let cts_response = transfer.next(dt).unwrap().expect("Response frame");
        assert!(matches!(&cts_response, Response::Cts(cts) if cts.next_sequence() == 3));

        // send third data transfer which should trigger a EndOfMsgAck response.
        let dt = message::DataTransfer::try_from([3, 1, 2, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF].as_ref())
            .unwrap();
        let ack_response = transfer.next(dt).unwrap().expect("Response frame");
        assert!(matches!(&ack_response, Response::End(end) if end.total_size() == 16));
        assert!(matches!(&ack_response, Response::End(end) if end.total_packets() == 3));

        assert_eq!(
            transfer.finished().unwrap(),
            &[1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2]
        );
    }

    fn dt(seq: u8, data: [u8; 7]) -> DataTransfer {
        let mut raw = [0u8; 8];
        raw[0] = seq;
        raw[1..].copy_from_slice(&data);
        DataTransfer::try_from(raw.as_ref()).unwrap()
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn finished_returns_none_before_complete() {
        let rts = RequestToSend::new(14, None, Pgn::Request);
        let transfer = Transfer::new(rts);
        assert!(transfer.finished().is_none());
    }

    #[test]
    fn transfer_with_borrowed_storage_full_roundtrip() {
        // 9 bytes = 2 packets, no CTS limit
        let pgn = Pgn::Request;
        let rts = RequestToSend::new(9, None, pgn);
        let mut buf = [0u8; 14]; // 2 * 7
        let mut transfer = Transfer::new_with_storage(rts, buf.as_mut());

        assert!(transfer.finished().is_none());

        // packet 1
        let resp = transfer
            .next(dt(1, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11]))
            .unwrap();
        assert!(resp.is_none(), "no response mid-transfer without CTS limit");

        // packet 2 - should return EndOfMsgAck
        let resp = transfer
            .next(dt(2, [0x22, 0x33, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]))
            .unwrap();
        assert!(matches!(resp, Some(Response::End(_))));

        let data = transfer.finished().unwrap();
        assert_eq!(
            data,
            &[0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11, 0x22, 0x33]
        );
    }

    #[test]
    fn transfer_with_borrowed_storage_cts_triggered() {
        // 16 bytes, max 2 packets per CTS window
        let pgn = Pgn::Request;
        let rts = RequestToSend::new(16, Some(2), pgn);
        let mut buf = [0u8; 21];
        let mut transfer = Transfer::new_with_storage(rts, buf.as_mut());

        transfer.next(dt(1, [1, 2, 3, 4, 5, 6, 7])).unwrap();

        let resp = transfer.next(dt(2, [8, 9, 10, 11, 12, 13, 14])).unwrap();
        assert!(
            matches!(&resp, Some(Response::Cts(cts)) if cts.next_sequence() == 3),
            "expected CTS after 2nd packet"
        );

        let resp = transfer
            .next(dt(3, [15, 16, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]))
            .unwrap();
        assert!(matches!(resp, Some(Response::End(_))));

        assert_eq!(
            transfer.finished().unwrap(),
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
        );
    }

    #[test]
    fn transfer_borrowed_storage_too_small_returns_error() {
        // buffer only fits 1 packet (7 bytes) but transfer needs 2
        let rts = RequestToSend::new(9, None, Pgn::Request);
        let mut buf = [0u8; 7];
        let mut transfer = Transfer::new_with_storage(rts, buf.as_mut());

        transfer.next(dt(1, [1, 2, 3, 4, 5, 6, 7])).unwrap();

        let err = transfer
            .next(dt(2, [8, 9, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]))
            .unwrap_err();
        assert!(matches!(err.0, Error::StorageTooSmall));
        assert_eq!(err.1.reason(), AbortReason::Custom);
        assert_eq!(err.1.sender_role(), AbortSenderRole::Receiver);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn out_of_order_packet_returns_sequence_error() {
        let rts = RequestToSend::new(14, None, Pgn::Request);
        let mut transfer = Transfer::new(rts);

        // send packet 2 before packet 1
        let err = transfer.next(dt(2, [1, 2, 3, 4, 5, 6, 7])).unwrap_err();
        assert!(matches!(err.0, Error::Sequence));
        assert_eq!(err.1.reason(), AbortReason::BadSequenceNumber);
        assert_eq!(err.1.sender_role(), AbortSenderRole::Receiver);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn duplicate_packet_returns_sequence_error() {
        let rts = RequestToSend::new(14, None, Pgn::Request);
        let mut transfer = Transfer::new(rts);

        transfer.next(dt(1, [1, 2, 3, 4, 5, 6, 7])).unwrap();
        // send packet 1 again instead of packet 2
        let err = transfer.next(dt(1, [1, 2, 3, 4, 5, 6, 7])).unwrap_err();
        assert!(matches!(err.0, Error::Sequence));
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn any_packet_after_abort_returns_previous_abort_error() {
        let rts = RequestToSend::new(14, None, Pgn::Request);
        let mut transfer = Transfer::new(rts);

        // trigger an abort with a bad sequence number
        transfer.next(dt(99, [0; 7])).unwrap_err();

        // any subsequent call should return PreviousAbort
        let err = transfer.next(dt(1, [0; 7])).unwrap_err();
        assert!(matches!(err.0, Error::PreviousAbort));
        assert_eq!(err.1.reason(), AbortReason::UnexpectedDataTransfer);
        assert_eq!(err.1.sender_role(), AbortSenderRole::Receiver);
    }

    #[test]
    fn response_cts_into_bytes() {
        let cts = ClearToSend::new(Some(5), 3, Pgn::Request);
        let resp = Response::Cts(cts.clone());
        let from_resp: [u8; 8] = (&resp).into();
        let from_cts: [u8; 8] = (&cts).into();
        assert_eq!(from_resp, from_cts);
    }

    #[test]
    fn response_end_into_bytes() {
        let end = EndOfMessageAck::new(16, 3, Pgn::Request);
        let resp = Response::End(end.clone());
        let from_resp: [u8; 8] = (&resp).into();
        let from_end: [u8; 8] = (&end).into();
        assert_eq!(from_resp, from_end);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn no_cts_when_no_limit() {
        // 3 packets, no CTS limit - only EndOfMsgAck at the end, nothing in between
        let rts = RequestToSend::new(21, None, Pgn::Request);
        let mut transfer = Transfer::new(rts);

        assert!(
            transfer
                .next(dt(1, [1, 2, 3, 4, 5, 6, 7]))
                .unwrap()
                .is_none()
        );
        assert!(
            transfer
                .next(dt(2, [8, 9, 10, 11, 12, 13, 14]))
                .unwrap()
                .is_none()
        );
        let resp = transfer.next(dt(3, [15, 16, 17, 18, 19, 20, 21])).unwrap();
        assert!(matches!(resp, Some(Response::End(_))));
    }
}
