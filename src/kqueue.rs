//! Bindings to kqueue (macOS, iOS, tvOS, watchOS, FreeBSD, NetBSD, OpenBSD, DragonFly BSD).

use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::time::Duration;

#[cfg(not(polling_no_io_safety))]
use std::os::unix::io::{AsFd, BorrowedFd};

use rustix::fd::OwnedFd;
use rustix::io::{fcntl_setfd, kqueue, Errno, FdFlags};

use crate::{Event, PollMode};

/// Interface to kqueue.
#[derive(Debug)]
pub struct Poller {
    /// File descriptor for the kqueue instance.
    kqueue_fd: OwnedFd,

    /// Notification pipe for waking up the poller.
    ///
    /// On platforms that support `EVFILT_USER`, this uses that to wake up the poller. Otherwise, it
    /// uses a pipe.
    notify: notify::Notify,
}

impl Poller {
    /// Creates a new poller.
    pub fn new() -> io::Result<Poller> {
        // Create a kqueue instance.
        let kqueue_fd = kqueue::kqueue()?;
        fcntl_setfd(&kqueue_fd, FdFlags::CLOEXEC)?;

        let poller = Poller {
            kqueue_fd,
            notify: notify::Notify::new()?,
        };

        // Register the notification pipe.
        poller.notify.register(&poller)?;

        log::trace!("new: kqueue_fd={:?}", poller.kqueue_fd);
        Ok(poller)
    }

    /// Whether this poller supports level-triggered events.
    pub fn supports_level(&self) -> bool {
        true
    }

    /// Whether this poller supports edge-triggered events.
    pub fn supports_edge(&self) -> bool {
        true
    }

    /// Adds a new file descriptor.
    pub fn add(&self, fd: RawFd, ev: Event, mode: PollMode) -> io::Result<()> {
        // File descriptors don't need to be added explicitly, so just modify the interest.
        self.modify(fd, ev, mode)
    }

    /// Modifies an existing file descriptor.
    pub fn modify(&self, fd: RawFd, ev: Event, mode: PollMode) -> io::Result<()> {
        if !self.notify.has_fd(fd) {
            log::trace!(
                "add: kqueue_fd={:?}, fd={}, ev={:?}",
                self.kqueue_fd,
                fd,
                ev
            );
        }

        let mode_flags = mode_to_flags(mode);

        let read_flags = if ev.readable {
            kqueue::EventFlags::ADD | mode_flags
        } else {
            kqueue::EventFlags::DELETE
        };
        let write_flags = if ev.writable {
            kqueue::EventFlags::ADD | mode_flags
        } else {
            kqueue::EventFlags::DELETE
        };

        // A list of changes for kqueue.
        let changelist = [
            kqueue::Event::new(
                kqueue::EventFilter::Read(fd),
                read_flags | kqueue::EventFlags::RECEIPT,
                ev.key as _,
            ),
            kqueue::Event::new(
                kqueue::EventFilter::Write(fd),
                write_flags | kqueue::EventFlags::RECEIPT,
                ev.key as _,
            ),
        ];

        // Apply changes.
        self.submit_changes(changelist)
    }

    /// Submit one or more changes to the kernel queue and check to see if they succeeded.
    pub(crate) fn submit_changes<A>(&self, changelist: A) -> io::Result<()>
    where
        A: Copy + AsRef<[kqueue::Event]> + AsMut<[kqueue::Event]>,
    {
        let mut eventlist = Vec::with_capacity(changelist.as_ref().len());

        // Apply changes.
        {
            let changelist = changelist.as_ref();

            unsafe {
                kqueue::kevent(&self.kqueue_fd, changelist, &mut eventlist, None)?;
            }
        }

        // Check for errors.
        for &ev in &eventlist {
            // TODO: Once the data field is exposed in rustix, use that.
            let data = unsafe { (*(&ev as *const kqueue::Event as *const libc::kevent)).data };

            // Explanation for ignoring EPIPE: https://github.com/tokio-rs/mio/issues/582
            if (ev.flags().contains(kqueue::EventFlags::ERROR))
                && data != 0
                && data != Errno::NOENT.raw_os_error() as _
                && data != Errno::PIPE.raw_os_error() as _
            {
                return Err(io::Error::from_raw_os_error(data as _));
            }
        }

        Ok(())
    }

    /// Deletes a file descriptor.
    pub fn delete(&self, fd: RawFd) -> io::Result<()> {
        // Simply delete interest in the file descriptor.
        self.modify(fd, Event::none(0), PollMode::Oneshot)
    }

    /// Waits for I/O events with an optional timeout.
    pub fn wait(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        log::trace!(
            "wait: kqueue_fd={:?}, timeout={:?}",
            self.kqueue_fd,
            timeout
        );

        // Wait for I/O events.
        let changelist = [];
        let eventlist = &mut events.list;
        let res = unsafe { kqueue::kevent(&self.kqueue_fd, &changelist, eventlist, timeout)? };

        log::trace!("new events: kqueue_fd={:?}, res={}", self.kqueue_fd, res);

        // Clear the notification (if received) and re-register interest in it.
        self.notify.reregister(self)?;

        Ok(())
    }

    /// Sends a notification to wake up the current or next `wait()` call.
    pub fn notify(&self) -> io::Result<()> {
        log::trace!("notify: kqueue_fd={:?}", self.kqueue_fd);
        self.notify.notify(self).ok();
        Ok(())
    }
}

impl AsRawFd for Poller {
    fn as_raw_fd(&self) -> RawFd {
        self.kqueue_fd.as_raw_fd()
    }
}

#[cfg(not(polling_no_io_safety))]
impl AsFd for Poller {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.kqueue_fd.as_fd()
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        log::trace!("drop: kqueue_fd={:?}", self.kqueue_fd);
        let _ = self.notify.deregister(self);
    }
}

/// A list of reported I/O events.
pub struct Events {
    list: Vec<kqueue::Event>,
}

unsafe impl Send for Events {}

impl Events {
    /// Creates an empty list.
    pub fn new() -> Events {
        Events {
            list: Vec::with_capacity(1024),
        }
    }

    /// Iterates over I/O events.
    pub fn iter(&self) -> impl Iterator<Item = Event> + '_ {
        // On some platforms, closing the read end of a pipe wakes up writers, but the
        // event is reported as EVFILT_READ with the EV_EOF flag.
        //
        // https://github.com/golang/go/commit/23aad448b1e3f7c3b4ba2af90120bde91ac865b4
        self.list.iter().map(|ev| Event {
            key: ev.udata() as usize,
            readable: matches!(
                ev.filter(),
                kqueue::EventFilter::Read(..)
                    | kqueue::EventFilter::Vnode { .. }
                    | kqueue::EventFilter::Proc { .. }
                    | kqueue::EventFilter::Signal { .. }
                    | kqueue::EventFilter::Timer { .. }
            ),
            writable: matches!(ev.filter(), kqueue::EventFilter::Write(..))
                || (matches!(ev.filter(), kqueue::EventFilter::Read(..))
                    && (ev.flags().intersects(kqueue::EventFlags::EOF))),
        })
    }
}

pub(crate) fn mode_to_flags(mode: PollMode) -> kqueue::EventFlags {
    use kqueue::EventFlags as EV;

    match mode {
        PollMode::Oneshot => EV::ONESHOT,
        PollMode::Level => EV::empty(),
        PollMode::Edge => EV::CLEAR,
        PollMode::EdgeOneshot => EV::ONESHOT | EV::CLEAR,
    }
}

#[cfg(any(
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "watchos",
))]
mod notify {
    use super::Poller;
    use rustix::io::kqueue;
    use std::io;
    use std::os::unix::io::RawFd;

    /// A notification pipe.
    ///
    /// This implementation uses `EVFILT_USER` to avoid allocating a pipe.
    #[derive(Debug)]
    pub(super) struct Notify;

    impl Notify {
        /// Creates a new notification pipe.
        pub(super) fn new() -> io::Result<Self> {
            Ok(Self)
        }

        /// Registers this notification pipe in the `Poller`.
        pub(super) fn register(&self, poller: &Poller) -> io::Result<()> {
            // Register an EVFILT_USER event.
            poller.submit_changes([kqueue::Event::new(
                kqueue::EventFilter::User {
                    ident: 0,
                    flags: kqueue::UserFlags::empty(),
                    user_flags: kqueue::UserDefinedFlags::new(0),
                },
                kqueue::EventFlags::ADD | kqueue::EventFlags::RECEIPT | kqueue::EventFlags::CLEAR,
                crate::NOTIFY_KEY as _,
            )])
        }

        /// Reregister this notification pipe in the `Poller`.
        pub(super) fn reregister(&self, _poller: &Poller) -> io::Result<()> {
            // We don't need to do anything, it's already registered as EV_CLEAR.
            Ok(())
        }

        /// Notifies the `Poller`.
        pub(super) fn notify(&self, poller: &Poller) -> io::Result<()> {
            // Trigger the EVFILT_USER event.
            poller.submit_changes([kqueue::Event::new(
                kqueue::EventFilter::User {
                    ident: 0,
                    flags: kqueue::UserFlags::TRIGGER,
                    user_flags: kqueue::UserDefinedFlags::new(0),
                },
                kqueue::EventFlags::ADD | kqueue::EventFlags::RECEIPT,
                crate::NOTIFY_KEY as _,
            )])?;

            Ok(())
        }

        /// Deregisters this notification pipe from the `Poller`.
        pub(super) fn deregister(&self, poller: &Poller) -> io::Result<()> {
            // Deregister the EVFILT_USER event.
            poller.submit_changes([kqueue::Event::new(
                kqueue::EventFilter::User {
                    ident: 0,
                    flags: kqueue::UserFlags::empty(),
                    user_flags: kqueue::UserDefinedFlags::new(0),
                },
                kqueue::EventFlags::DELETE | kqueue::EventFlags::RECEIPT,
                crate::NOTIFY_KEY as _,
            )])
        }

        /// Whether this raw file descriptor is associated with this pipe.
        pub(super) fn has_fd(&self, _fd: RawFd) -> bool {
            false
        }
    }
}

#[cfg(not(any(
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "watchos",
)))]
mod notify {
    use super::Poller;
    use crate::{Event, PollMode, NOTIFY_KEY};
    use std::io::{self, prelude::*};
    use std::os::unix::{
        io::{AsRawFd, RawFd},
        net::UnixStream,
    };

    /// A notification pipe.
    ///
    /// This implementation uses a pipe to send notifications.
    #[derive(Debug)]
    pub(super) struct Notify {
        /// The read end of the pipe.
        read_stream: UnixStream,

        /// The write end of the pipe.
        write_stream: UnixStream,
    }

    impl Notify {
        /// Creates a new notification pipe.
        pub(super) fn new() -> io::Result<Self> {
            let (read_stream, write_stream) = UnixStream::pair()?;
            read_stream.set_nonblocking(true)?;
            write_stream.set_nonblocking(true)?;

            Ok(Self {
                read_stream,
                write_stream,
            })
        }

        /// Registers this notification pipe in the `Poller`.
        pub(super) fn register(&self, poller: &Poller) -> io::Result<()> {
            // Register the read end of this pipe.
            poller.add(
                self.read_stream.as_raw_fd(),
                Event::readable(NOTIFY_KEY),
                PollMode::Oneshot,
            )
        }

        /// Reregister this notification pipe in the `Poller`.
        pub(super) fn reregister(&self, poller: &Poller) -> io::Result<()> {
            // Clear out the notification.
            while (&self.read_stream).read(&mut [0; 64]).is_ok() {}

            // Reregister the read end of this pipe.
            poller.modify(
                self.read_stream.as_raw_fd(),
                Event::readable(NOTIFY_KEY),
                PollMode::Oneshot,
            )
        }

        /// Notifies the `Poller`.
        #[allow(clippy::unused_io_amount)]
        pub(super) fn notify(&self, _poller: &Poller) -> io::Result<()> {
            // Write to the write end of the pipe
            (&self.write_stream).write(&[1])?;

            Ok(())
        }

        /// Deregisters this notification pipe from the `Poller`.
        pub(super) fn deregister(&self, poller: &Poller) -> io::Result<()> {
            // Deregister the read end of the pipe.
            poller.delete(self.read_stream.as_raw_fd())
        }

        /// Whether this raw file descriptor is associated with this pipe.
        pub(super) fn has_fd(&self, fd: RawFd) -> bool {
            self.read_stream.as_raw_fd() == fd
        }
    }
}
