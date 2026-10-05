//! Launcher <-> elevated GUI broker: a closed set of user-context actions.
//!
//! OWNER: platform agent. Protocol (spec §3.1): request `[kind, arg]`,
//! response `[status]`. No strings cross the boundary.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    OpenWindowsUpdate,
    OpenWindowsSecurity,
    OpenEncryption,
    OpenSignIn,
    InstallBitwarden,
    /// Turn off silent sponsored-app installs for the signed-in user (HKCU).
    BlockSuggestedApps,
    /// Reinstall a removed app; arg = index into `secblitz::debloat::catalog()`.
    ReinstallStoreApp(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    Done,
    Failed,
    /// Restore fell back to opening the Store page for the user.
    OpenedStore,
    Unavailable,
}

impl Request {
    pub fn encode(self) -> [u8; 3] {
        // TODO(platform): [kind, arg_lo, arg_hi]
        [0, 0, 0]
    }
    pub fn decode(bytes: [u8; 3]) -> Option<Self> {
        // TODO(platform): reject unknown kinds and out-of-range catalog indices
        let _ = bytes;
        None
    }
}

/// Elevated-GUI side. `None` when the GUI was started without a broker.
#[derive(Debug)]
pub struct Client {
    _private: (),
}

impl Client {
    /// Connect to `\\.\pipe\secblitz-broker-<id>`; `id` must be 32 lowercase hex.
    pub fn connect(id: &str) -> anyhow::Result<Self> {
        let _ = id;
        anyhow::bail!("broker not implemented")
    }
    /// Blocking round trip. Call from a background task, never the UI thread.
    pub fn send(&self, request: Request) -> anyhow::Result<Reply> {
        let _ = request;
        anyhow::bail!("broker not implemented")
    }
}
