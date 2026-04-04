use crate::{
    CanError, ControllerError, Frame, ProtocolErrorKind, ProtocolErrorLocation, TransceiverError,
};
use async_io::Async;
use std::ffi::{CString, c_int};
use std::io::ErrorKind;
use std::mem::MaybeUninit;
use std::os::fd::{AsFd, BorrowedFd, RawFd};

/// Error type.
#[derive(Debug)]
pub enum Error {
    /// Bus error.
    Can(CanError),
    /// IO error.
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Can(err) => write!(f, "CAN Error: {:?}", err),
            Self::Io(err) => write!(f, "IO Error: {}", err),
        }
    }
}

impl embedded_can::Error for Error {
    fn kind(&self) -> embedded_can::ErrorKind {
        match self {
            // it's not possible to assign a specific kind since multiple can be
            // present at one time.
            Self::Can(_) => embedded_can::ErrorKind::Other,
            Self::Io(err) => match err.kind() {
                ErrorKind::WouldBlock => embedded_can::ErrorKind::Overrun,
                ErrorKind::TimedOut => embedded_can::ErrorKind::Overrun,
                _ => embedded_can::ErrorKind::Other,
            },
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<CanError> for Error {
    fn from(err: CanError) -> Self {
        Self::Can(err)
    }
}

struct LibcSocket(RawFd);

unsafe impl Send for LibcSocket {}

impl AsFd for LibcSocket {
    fn as_fd(&self) -> std::os::unix::prelude::BorrowedFd<'_> {
        unsafe { BorrowedFd::borrow_raw(self.0) }
    }
}

/// CAN Bus interface socket.
///
/// This provides both a blocking and async interface to a SocketCAN device.
///
/// ```
/// # use cansocket::Socket;
/// # async fn example() -> Result<(), cansocket::Error> {
/// // Open a socket.
/// let socket = Socket::new("can0")?;
/// // Receive a frame.
/// let frame = socket.recv().await?;
/// # Ok(())
/// # }
/// ```
pub struct Socket {
    fd: Async<LibcSocket>,
}

impl Socket {
    pub fn new(iface: &str) -> std::io::Result<Self> {
        let fd = check_return(unsafe {
            libc::socket(
                libc::PF_CAN,
                libc::SOCK_RAW | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
                libc::CAN_RAW,
            )
        })?;

        let index = unsafe { libc::if_nametoindex(CString::new(iface)?.as_ptr()) };
        if index == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut addr: libc::sockaddr_can = unsafe { std::mem::zeroed() };
        addr.can_family = libc::AF_CAN as _;
        addr.can_ifindex = index as _;

        check_return(unsafe {
            libc::bind(
                fd,
                &addr as *const _ as *const _,
                std::mem::size_of_val(&addr) as _,
            )
        })?;

        Ok(Self {
            fd: Async::new(LibcSocket(fd))?,
        })
    }

    /// Enable receiving protocol errors as [`Error::Can`].
    pub fn protocol_errors(&self, enabled: bool) -> std::io::Result<()> {
        let err_mask = if enabled {
            libc::CAN_ERR_TX_TIMEOUT
                | libc::CAN_ERR_LOSTARB
                | libc::CAN_ERR_CRTL
                | libc::CAN_ERR_PROT
                | libc::CAN_ERR_TRX
                | libc::CAN_ERR_ACK
                | libc::CAN_ERR_BUSOFF
                | libc::CAN_ERR_BUSERROR
                | libc::CAN_ERR_RESTARTED
                | libc::CAN_ERR_CNT
        } else {
            0
        };

        check_return(unsafe {
            libc::setsockopt(
                self.fd.get_ref().0,
                libc::SOL_CAN_RAW,
                libc::CAN_RAW_ERR_FILTER,
                &err_mask as *const _ as *const _,
                std::mem::size_of_val(&err_mask) as _,
            )
        })
        .map(|_| {})
    }

    /// Enable other socket listeners receiving sent frames.
    pub fn loopback(&self, enabled: bool) -> std::io::Result<()> {
        let loopback = c_int::from(enabled);
        check_return(unsafe {
            libc::setsockopt(
                self.fd.get_ref().0,
                libc::SOL_CAN_RAW,
                libc::CAN_RAW_LOOPBACK,
                &loopback as *const _ as *const _,
                std::mem::size_of_val(&loopback) as _,
            )
        })
        .map(|_| {})
    }

    /// Enable receiving own sent frames.
    pub fn recv_own_msgs(&self, enabled: bool) -> std::io::Result<()> {
        let recv_own_msgs = c_int::from(enabled);
        check_return(unsafe {
            libc::setsockopt(
                self.fd.get_ref().0,
                libc::SOL_CAN_RAW,
                libc::CAN_RAW_RECV_OWN_MSGS,
                &recv_own_msgs as *const _ as *const _,
                std::mem::size_of_val(&recv_own_msgs) as _,
            )
        })
        .map(|_| {})
    }

    /// Async send frame.
    pub async fn send(&self, frame: &Frame) -> Result<(), Error> {
        Ok(self
            .fd
            .write_with(|write| {
                check_return(unsafe {
                    libc::send(
                        write.0,
                        &frame.0 as *const _ as *const _,
                        std::mem::size_of::<libc::can_frame>(),
                        0,
                    )
                })
                .map(|_| {})
            })
            .await?)
    }

    /// Blocking send frame.
    pub fn send_blocking(&self, frame: &Frame) -> Result<(), Error> {
        loop {
            unsafe {
                match check_return(libc::send(
                    self.fd.get_ref().0,
                    &frame.0 as *const _ as *const _,
                    std::mem::size_of::<libc::can_frame>(),
                    0,
                )) {
                    Ok(_) => return Ok(()),
                    Err(err) => match err.kind() {
                        ErrorKind::WouldBlock => continue,
                        _ => return Err(err.into()),
                    },
                }
            }
        }
    }

    /// Async receive frame.
    pub async fn recv(&self) -> Result<Frame, Error> {
        let mut frame: MaybeUninit<libc::can_frame> = MaybeUninit::uninit();
        let frame = self
            .fd
            .read_with(|read| {
                check_return(unsafe {
                    libc::recv(
                        read.0,
                        frame.as_mut_ptr() as *mut _,
                        std::mem::size_of::<libc::can_frame>(),
                        0,
                    )
                })?;
                Ok(unsafe { frame.assume_init() })
            })
            .await?;
        check_error_frame(&frame)?;
        Ok(Frame(frame))
    }

    /// Blocking receive.
    ///
    /// Busy-waits for a frame to arrive.
    pub fn recv_blocking(&self) -> Result<Frame, Error> {
        loop {
            let mut frame: MaybeUninit<libc::can_frame> = MaybeUninit::uninit();
            match check_return(unsafe {
                libc::recv(
                    self.fd.get_ref().0,
                    &mut frame as *mut _ as *mut _,
                    std::mem::size_of::<libc::can_frame>(),
                    0,
                )
            }) {
                Ok(_) => {
                    let frame = unsafe { frame.assume_init() };
                    check_error_frame(&frame)?;
                    return Ok(Frame(frame));
                }
                Err(err) => match err.kind() {
                    ErrorKind::WouldBlock => continue,
                    _ => return Err(err.into()),
                },
            }
        }
    }
}

impl embedded_can::blocking::Can for Socket {
    type Frame = crate::Frame;
    type Error = Error;

    fn receive(&mut self) -> std::result::Result<Self::Frame, Self::Error> {
        self.recv_blocking()
    }

    fn transmit(&mut self, frame: &Self::Frame) -> std::result::Result<(), Self::Error> {
        self.send_blocking(frame)
    }
}

/// Transforms an possibly error number into last OS error.
fn check_return<T>(return_value: T) -> std::io::Result<T>
where
    T: PartialEq + From<i8>,
{
    if return_value == T::from(-1) {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(return_value)
    }
}

fn check_error_frame(frame: &libc::can_frame) -> Result<(), CanError> {
    if (frame.can_id & libc::CAN_ERR_FLAG) == 0 {
        return Ok(());
    }

    Err(CanError {
        tx_timeout: (frame.can_id & libc::CAN_ERR_TX_TIMEOUT) != 0,
        lost_arbitration: if (frame.can_id & libc::CAN_ERR_LOSTARB) != 0 {
            Some(frame.data[0])
        } else {
            None
        },
        controller: if (frame.can_id & libc::CAN_ERR_CRTL) != 0 {
            Some(ControllerError(frame.data[1]))
        } else {
            None
        },
        protocol: if (frame.can_id & libc::CAN_ERR_PROT) != 0 {
            Some((
                ProtocolErrorKind(frame.data[2]),
                ProtocolErrorLocation::try_from(frame.data[3]),
            ))
        } else {
            None
        },
        transceiver: if (frame.can_id & libc::CAN_ERR_TRX) != 0 {
            Some(TransceiverError(frame.data[4]))
        } else {
            None
        },
        no_ack: (frame.can_id & libc::CAN_ERR_ACK) != 0,
        bus_off: (frame.can_id & libc::CAN_ERR_BUSOFF) != 0,
        bus_error: (frame.can_id & libc::CAN_ERR_BUSERROR) != 0,
        restarted: (frame.can_id & libc::CAN_ERR_RESTARTED) != 0,
        tx_rx_error_count: if (frame.can_id & libc::CAN_ERR_CNT) != 0 {
            Some((frame.data[6], frame.data[7]))
        } else {
            None
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_return() {
        assert!(check_return(-1).is_err());
        assert!(check_return(0).is_ok());
        assert!(check_return(1).is_ok());
    }
}
