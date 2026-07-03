//! Stream capability and security policy model.
//!
//! The kernel defines the capability contract (`CapabilityName`) and the
//! expression graph (`Expr`); this module supplies the concrete streaming-fabric
//! capabilities a stream may exercise and the policy that bounds remote access
//! and redacts sensitive payloads. [`StreamSecurityCapability`] names the gated
//! operations, [`StreamRemoteLimits`] bounds what a remote boundary may carry,
//! [`StreamSecurityPolicy`] inspects expressions for leaked secrets, and
//! [`StreamRedactionFinding`] records what a redaction scan caught.

use sim_kernel::{CapabilityName, Error, Expr, Result, Symbol};

use crate::{StreamCapability, TransportProfile};

/// Concrete capability a stream may exercise, gated against the kernel
/// capability contract.
///
/// Each variant names one stream operation or remote surface; the runtime
/// checks the corresponding [`CapabilityName`] before allowing the action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamSecurityCapability {
    /// Open a stream.
    Open,
    /// Read frames from a stream.
    Read,
    /// Push frames into a stream.
    Push,
    /// Cancel an active stream.
    Cancel,
    /// Read stream statistics.
    Stats,
    /// Preview a stream across a remote boundary.
    RemotePreview,
    /// Render a stream across a remote boundary.
    RemoteRender,
    /// Access LAN MIDI transport.
    LanMidi,
    /// Access a host audio/MIDI device.
    HostDevice,
    /// Cross a remote network boundary.
    RemoteNetwork,
}

impl StreamSecurityCapability {
    /// Returns the stable dotted wire label for this capability (for example
    /// `stream.open`).
    pub fn wire_label(self) -> &'static str {
        match self {
            Self::Open => "stream.open",
            Self::Read => "stream.read",
            Self::Push => "stream.push",
            Self::Cancel => "stream.cancel",
            Self::Stats => "stream.stats",
            Self::RemotePreview => "stream.remote.preview",
            Self::RemoteRender => "stream.remote.render",
            Self::LanMidi => "stream.lan.midi",
            Self::HostDevice => "stream.host.device",
            Self::RemoteNetwork => "stream.remote.network",
        }
    }

    /// Returns the kernel [`CapabilityName`] this capability checks against.
    pub fn capability(self) -> CapabilityName {
        CapabilityName::new(self.wire_label())
    }

    /// Returns the qualified symbol naming this capability in the
    /// `stream/security-capability` namespace.
    pub fn symbol(self) -> Symbol {
        Symbol::qualified("stream/security-capability", self.wire_label())
    }
}

/// Returns the capability name gating opening a stream.
pub fn stream_open_capability() -> CapabilityName {
    StreamSecurityCapability::Open.capability()
}

/// Returns the capability name gating reading from a stream.
pub fn stream_read_capability() -> CapabilityName {
    StreamSecurityCapability::Read.capability()
}

/// Returns the capability name gating pushing into a stream.
pub fn stream_push_capability() -> CapabilityName {
    StreamSecurityCapability::Push.capability()
}

/// Returns the capability name gating cancelling a stream.
pub fn stream_cancel_capability() -> CapabilityName {
    StreamSecurityCapability::Cancel.capability()
}

/// Returns the capability name gating reading stream statistics.
pub fn stream_stats_capability() -> CapabilityName {
    StreamSecurityCapability::Stats.capability()
}

/// Returns the capability name gating remote stream preview.
pub fn stream_remote_preview_capability() -> CapabilityName {
    StreamSecurityCapability::RemotePreview.capability()
}

/// Returns the capability name gating remote stream render.
pub fn stream_remote_render_capability() -> CapabilityName {
    StreamSecurityCapability::RemoteRender.capability()
}

/// Returns the capability name gating LAN MIDI access.
pub fn stream_lan_midi_capability() -> CapabilityName {
    StreamSecurityCapability::LanMidi.capability()
}

/// Returns the capability name gating host device access.
pub fn stream_host_device_capability() -> CapabilityName {
    StreamSecurityCapability::HostDevice.capability()
}

/// Returns the capability name gating remote network access.
pub fn stream_remote_network_capability() -> CapabilityName {
    StreamSecurityCapability::RemoteNetwork.capability()
}

/// Returns the full set of stream security capabilities in declaration order.
pub fn stream_security_capabilities() -> [StreamSecurityCapability; 10] {
    [
        StreamSecurityCapability::Open,
        StreamSecurityCapability::Read,
        StreamSecurityCapability::Push,
        StreamSecurityCapability::Cancel,
        StreamSecurityCapability::Stats,
        StreamSecurityCapability::RemotePreview,
        StreamSecurityCapability::RemoteRender,
        StreamSecurityCapability::LanMidi,
        StreamSecurityCapability::HostDevice,
        StreamSecurityCapability::RemoteNetwork,
    ]
}

/// Returns the kernel capability names for every stream security capability.
pub fn stream_security_capability_names() -> Vec<CapabilityName> {
    stream_security_capabilities()
        .into_iter()
        .map(StreamSecurityCapability::capability)
        .collect()
}

/// Quantitative bounds a stream must respect when it crosses a remote boundary.
///
/// These limits cap payload size, frame count, concurrency, lifetime, and rate
/// so a remote peer cannot exhaust local resources.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamRemoteLimits {
    /// Maximum bytes carried in a single frame payload.
    pub max_frame_payload_bytes: usize,
    /// Maximum number of frames a stream may carry.
    pub max_stream_frames: usize,
    /// Maximum number of frames in flight at once.
    pub max_inflight_frames: usize,
    /// Maximum stream lifetime in milliseconds.
    pub max_duration_ms: u64,
    /// Maximum frame rate in hertz.
    pub max_rate_hz: u32,
    /// Maximum bytes carried in a single binary payload.
    pub max_binary_payload_bytes: usize,
}

impl Default for StreamRemoteLimits {
    fn default() -> Self {
        Self {
            max_frame_payload_bytes: 1024 * 1024,
            max_stream_frames: 1024,
            max_inflight_frames: 64,
            max_duration_ms: 60_000,
            max_rate_hz: 120,
            max_binary_payload_bytes: 256 * 1024,
        }
    }
}

impl StreamRemoteLimits {
    /// Checks that every positive-only limit is non-zero.
    ///
    /// Returns an [`Error::Eval`] naming the first limit that is zero.
    pub fn validate(self) -> Result<()> {
        if self.max_frame_payload_bytes == 0 {
            return Err(Error::Eval(
                "stream remote frame-size limit must be positive".to_owned(),
            ));
        }
        if self.max_duration_ms == 0 {
            return Err(Error::Eval(
                "stream remote duration limit must be positive".to_owned(),
            ));
        }
        if self.max_rate_hz == 0 {
            return Err(Error::Eval(
                "stream remote rate limit must be positive".to_owned(),
            ));
        }
        if self.max_binary_payload_bytes == 0 {
            return Err(Error::Eval(
                "stream remote binary payload limit must be positive".to_owned(),
            ));
        }
        Ok(())
    }

    /// Validates these limits against a transport profile.
    ///
    /// Beyond [`validate`](Self::validate), this rejects a realtime profile that
    /// lacks local preview transport and a remote profile that crosses a
    /// boundary without bounded limits.
    pub fn validate_profile(self, profile: &TransportProfile) -> Result<()> {
        self.validate()?;
        if profile.has_capability(StreamCapability::Realtime)
            && !profile.has_capability(StreamCapability::Preview)
        {
            return Err(Error::Eval(format!(
                "stream profile {} requires local realtime transport",
                profile.name()
            )));
        }
        if profile.has_capability(StreamCapability::Remote)
            && !profile.has_capability(StreamCapability::Bounded)
        {
            return Err(Error::Eval(format!(
                "stream profile {} crosses a remote boundary without bounded limits",
                profile.name()
            )));
        }
        Ok(())
    }

    /// Returns the effective frame ceiling: the smaller of the configured frame
    /// cap and the frames implied by the duration and rate limits.
    pub fn effective_frame_limit(self) -> usize {
        let rate_duration = (self.max_duration_ms as u128)
            .saturating_mul(self.max_rate_hz as u128)
            .div_ceil(1000);
        self.max_stream_frames
            .min(rate_duration.max(1).min(usize::MAX as u128) as usize)
    }

    /// Encodes these limits as a map expression keyed by limit name.
    pub fn to_expr(self) -> Expr {
        Expr::Map(vec![
            field(
                "max-frame-payload-bytes",
                self.max_frame_payload_bytes.to_string(),
            ),
            field("max-stream-frames", self.max_stream_frames.to_string()),
            field("max-inflight-frames", self.max_inflight_frames.to_string()),
            field("max-duration-ms", self.max_duration_ms.to_string()),
            field("max-rate-hz", self.max_rate_hz.to_string()),
            field(
                "max-binary-payload-bytes",
                self.max_binary_payload_bytes.to_string(),
            ),
        ])
    }
}

/// Category of sensitive content a redaction scan can flag in a payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamRedactionFinding {
    /// A private user path (for example a home directory).
    PrivatePath,
    /// A host name or URL.
    HostName,
    /// An absolute filesystem path.
    AbsolutePath,
    /// A credential, token, or secret.
    Credential,
    /// A patch-bank or sysex-bank payload.
    PatchBankPayload,
    /// A binary payload larger than the configured limit.
    LargeBinaryData,
}

impl StreamRedactionFinding {
    /// Returns the stable wire label for this finding category.
    pub fn wire_label(self) -> &'static str {
        match self {
            Self::PrivatePath => "private-path",
            Self::HostName => "host-name",
            Self::AbsolutePath => "absolute-path",
            Self::Credential => "credential",
            Self::PatchBankPayload => "patch-bank-payload",
            Self::LargeBinaryData => "large-binary-data",
        }
    }

    /// Returns the qualified symbol naming this finding in the
    /// `stream/redaction` namespace.
    pub fn symbol(self) -> Symbol {
        Symbol::qualified("stream/redaction", self.wire_label())
    }
}

/// Returns the qualified symbols for every redaction finding category.
pub fn stream_redaction_finding_symbols() -> [Symbol; 6] {
    [
        StreamRedactionFinding::PrivatePath.symbol(),
        StreamRedactionFinding::HostName.symbol(),
        StreamRedactionFinding::AbsolutePath.symbol(),
        StreamRedactionFinding::Credential.symbol(),
        StreamRedactionFinding::PatchBankPayload.symbol(),
        StreamRedactionFinding::LargeBinaryData.symbol(),
    ]
}

/// Security policy applied to stream payloads that leave the local boundary.
///
/// Combines the [`StreamRemoteLimits`] bounds with a redaction scan over
/// expression graphs so private paths, host names, credentials, and oversized
/// binaries do not escape in public payloads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamSecurityPolicy {
    /// Bounds applied to remote stream access.
    pub remote_limits: StreamRemoteLimits,
}

impl StreamSecurityPolicy {
    /// Rejects an expression destined for a public surface if it contains any
    /// redaction finding.
    ///
    /// Returns an [`Error::Eval`] naming the offending finding category.
    pub fn validate_public_expr(self, expr: &Expr) -> Result<()> {
        if let Some(finding) = self.finding_for_expr(expr) {
            return Err(Error::Eval(format!(
                "stream public payload contains {}",
                finding.wire_label()
            )));
        }
        Ok(())
    }

    /// Recursively scans an expression graph and returns the first redaction
    /// finding, or `None` if the expression is clean.
    pub fn finding_for_expr(self, expr: &Expr) -> Option<StreamRedactionFinding> {
        match expr {
            Expr::Symbol(symbol) | Expr::Local(symbol) => {
                self.finding_for_text(&symbol.as_qualified_str())
            }
            Expr::String(value) => self.finding_for_text(value),
            Expr::Bytes(bytes) if bytes.len() > self.remote_limits.max_binary_payload_bytes => {
                Some(StreamRedactionFinding::LargeBinaryData)
            }
            Expr::List(items) | Expr::Vector(items) | Expr::Set(items) | Expr::Block(items) => {
                items.iter().find_map(|item| self.finding_for_expr(item))
            }
            Expr::Map(entries) => entries.iter().find_map(|(key, value)| {
                self.finding_for_expr(key)
                    .or_else(|| self.finding_for_expr(value))
            }),
            Expr::Call { operator, args } => self
                .finding_for_expr(operator)
                .or_else(|| args.iter().find_map(|arg| self.finding_for_expr(arg))),
            Expr::Infix {
                operator,
                left,
                right,
            } => self
                .finding_for_text(&operator.as_qualified_str())
                .or_else(|| self.finding_for_expr(left))
                .or_else(|| self.finding_for_expr(right)),
            Expr::Prefix { operator, arg } | Expr::Postfix { operator, arg } => self
                .finding_for_text(&operator.as_qualified_str())
                .or_else(|| self.finding_for_expr(arg)),
            Expr::Quote { expr, .. } => self.finding_for_expr(expr),
            Expr::Annotated { expr, annotations } => self.finding_for_expr(expr).or_else(|| {
                annotations.iter().find_map(|(key, value)| {
                    self.finding_for_text(&key.as_qualified_str())
                        .or_else(|| self.finding_for_expr(value))
                })
            }),
            Expr::Extension { tag, payload } => self
                .finding_for_text(&tag.as_qualified_str())
                .or_else(|| self.finding_for_expr(payload)),
            _ => None,
        }
    }

    /// Scans a single text value and returns the first redaction finding, or
    /// `None` if no sensitive pattern matches.
    pub fn finding_for_text(self, value: &str) -> Option<StreamRedactionFinding> {
        let lower = value.to_ascii_lowercase();
        if contains_credential(&lower) {
            return Some(StreamRedactionFinding::Credential);
        }
        if contains_patch_bank(&lower) {
            return Some(StreamRedactionFinding::PatchBankPayload);
        }
        if contains_host_name(value, &lower) {
            return Some(StreamRedactionFinding::HostName);
        }
        if contains_private_path(value, &lower) {
            return Some(StreamRedactionFinding::PrivatePath);
        }
        if contains_absolute_path(value) {
            return Some(StreamRedactionFinding::AbsolutePath);
        }
        None
    }
}

fn contains_credential(lower: &str) -> bool {
    lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("auth-token")
        || lower.contains("bearer ")
        || lower.contains("credential")
        || lower.contains("password")
        || lower.contains("secret")
        || lower.contains("token=")
}

fn contains_patch_bank(lower: &str) -> bool {
    lower.contains("patch-bank")
        || lower.contains("patch_bank")
        || lower.contains("sysex-bank")
        || lower.contains("sysex_bank")
}

fn contains_host_name(_value: &str, lower: &str) -> bool {
    lower.contains("hostname=")
        || lower.contains("host=")
        || lower.contains("http://")
        || lower.contains("https://")
        || lower.contains("ws://")
        || lower.contains("wss://")
        || lower.contains(".local")
        || lower.contains(".lan")
}

fn contains_private_path(value: &str, lower: &str) -> bool {
    lower.contains("/home/")
        || lower.contains("/users/")
        || lower.contains("\\users\\")
        || lower.contains("/private/")
        || lower.contains("private/")
        || lower.contains("private-path")
        || value.starts_with('~')
}

fn contains_absolute_path(value: &str) -> bool {
    value.starts_with('/') || looks_like_windows_absolute_path(value)
}

fn looks_like_windows_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() > 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

fn field(name: &str, value: String) -> (Expr, Expr) {
    (Expr::Symbol(Symbol::new(name)), Expr::String(value))
}
