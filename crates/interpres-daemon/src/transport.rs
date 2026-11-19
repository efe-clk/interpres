//! Where the daemon listens, and the platform-specific listen/connect code.

use std::io;
use std::path::PathBuf;

use tokio::io::{AsyncRead, AsyncWrite};

/// A Unix socket path, or a Windows named pipe name (`\\.\pipe\...`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint(pub PathBuf);

/// `$XDG_RUNTIME_DIR/interpres.sock` (private to the user), falling back to the temp
/// directory; on Windows `\\.\pipe\interpres-<user>`.
pub fn default_endpoint() -> Endpoint {
    #[cfg(windows)]
    {
        let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
        Endpoint(PathBuf::from(format!(r"\\.\pipe\interpres-{user}")))
    }
    #[cfg(not(windows))]
    {
        let dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        Endpoint(dir.join("interpres.sock"))
    }
}

pub trait Stream: AsyncRead + AsyncWrite + Send + Unpin + 'static {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin + 'static> Stream for T {}

pub async fn connect(ep: &Endpoint) -> io::Result<Box<dyn Stream>> {
    #[cfg(windows)]
    {
        // ERROR_PIPE_BUSY: every listening instance is taken; one frees up shortly.
        const ERROR_PIPE_BUSY: i32 = 231;
        let mut attempts = 0;
        loop {
            match tokio::net::windows::named_pipe::ClientOptions::new().open(&ep.0) {
                Ok(pipe) => return Ok(Box::new(pipe)),
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) && attempts < 20 => {
                    attempts += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
    #[cfg(not(windows))]
    {
        Ok(Box::new(tokio::net::UnixStream::connect(&ep.0).await?))
    }
}

/// Accepts connections one at a time.
pub struct Listener {
    #[cfg(not(windows))]
    inner: tokio::net::UnixListener,
    #[cfg(windows)]
    name: PathBuf,
    #[cfg(windows)]
    next: tokio::net::windows::named_pipe::NamedPipeServer,
}

impl Listener {
    pub fn bind(ep: &Endpoint) -> io::Result<Self> {
        #[cfg(windows)]
        {
            let next = tokio::net::windows::named_pipe::ServerOptions::new()
                .first_pipe_instance(true)
                .create(&ep.0)?;
            Ok(Self {
                name: ep.0.clone(),
                next,
            })
        }
        #[cfg(not(windows))]
        {
            // A leftover socket file from a crashed daemon would block bind.
            let _ = std::fs::remove_file(&ep.0);
            Ok(Self {
                inner: tokio::net::UnixListener::bind(&ep.0)?,
            })
        }
    }

    pub async fn accept(&mut self) -> io::Result<Box<dyn Stream>> {
        #[cfg(windows)]
        {
            // Clients arriving before the next instance exists get ERROR_PIPE_BUSY and
            // retry (see `connect`).
            self.next.connect().await?;
            let fresh = tokio::net::windows::named_pipe::ServerOptions::new().create(&self.name)?;
            let connected = std::mem::replace(&mut self.next, fresh);
            Ok(Box::new(connected))
        }
        #[cfg(not(windows))]
        {
            let (stream, _) = self.inner.accept().await?;
            Ok(Box::new(stream))
        }
    }
}
