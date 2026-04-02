use crate::Frame;
use async_io::Async;
use std::ffi::{CString, c_int};
use std::io::ErrorKind;
use std::mem::MaybeUninit;
use std::os::fd::{AsFd, BorrowedFd, RawFd};

/// Error type.
#[derive(Debug)]
pub enum Error {
    /// Protocol error.
    Can(embedded_can::ErrorKind),
    /// IO error.
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Can(kind) => write!(f, "CAN Error: {}", kind),
            Self::Io(err) => write!(f, "IO Error: {}", err),
        }
    }
}

impl embedded_can::Error for Error {
    fn kind(&self) -> embedded_can::ErrorKind {
        match self {
            Self::Can(can) => can.kind(),
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

impl From<embedded_can::ErrorKind> for Error {
    fn from(err: embedded_can::ErrorKind) -> Self {
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
/// // Open a socket.
/// let socket = Socket::new("can0")?;
/// // Receive a frame.
/// let frame = socket.recv().await?;
/// ```
///
/// Protocol errors are represented with the error type [`Error::Can`].
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

    /// Enable other socket listeners receiving sent frames.
    pub fn set_loopback(&self, enabled: bool) -> std::io::Result<()> {
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
    pub fn set_recv_own_msgs(&self, enabled: bool) -> std::io::Result<()> {
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

fn check_error_frame(frame: &libc::can_frame) -> Result<(), embedded_can::ErrorKind> {
    const CAN_ERR_FLAG: u32 = 0x20000000;
    const CAN_ERR_MASK: u32 = 0x1FFFFFFF;

    if (frame.can_id & CAN_ERR_FLAG) == 0 {
        return Ok(());
    }

    let error_class = frame.can_id & CAN_ERR_MASK;

    const CAN_ERR_TX_TIMEOUT: u32 = 0x00000001;
    const CAN_ERR_CRTL: u32 = 0x00000004;
    const CAN_ERR_PROT: u32 = 0x00000008;
    const CAN_ERR_ACK: u32 = 0x00000020;

    let error_kind = match error_class {
        CAN_ERR_TX_TIMEOUT => embedded_can::ErrorKind::Overrun,
        CAN_ERR_ACK => embedded_can::ErrorKind::Acknowledge,
        CAN_ERR_CRTL => {
            if frame.data[1] & 0x01 != 0 {
                embedded_can::ErrorKind::Overrun
            } else {
                embedded_can::ErrorKind::Other
            }
        }
        CAN_ERR_PROT => {
            if frame.data[3] & 0x01 != 0 {
                embedded_can::ErrorKind::Bit
            } else if frame.data[3] & 0x02 != 0 {
                embedded_can::ErrorKind::Form
            } else if frame.data[3] & 0x04 != 0 {
                embedded_can::ErrorKind::Stuff
            } else if frame.data[3] & 0x10 != 0 {
                embedded_can::ErrorKind::Crc
            } else {
                embedded_can::ErrorKind::Other
            }
        }
        _ => embedded_can::ErrorKind::Other,
    };

    Err(error_kind)
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
