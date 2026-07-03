//! Transport capability and latency model carried by a stream envelope.
//!
//! A [`TransportProfile`] names what a transport is allowed to do with a stream
//! ([`StreamCapability`]) together with the real-time [`LatencyClass`] it
//! promises. The kernel owns the capability/latency contract vocabulary as
//! [`Symbol`]s; this module supplies the concrete profile set, the
//! capability/latency consistency rules, and the named profile presets used
//! across the fabric (in-memory, real-time audio, buffered preview, and the LAN
//! and remote variants).

use sim_kernel::{Error, Expr, Result, Symbol};

use crate::buffer::{expr_kind, field, symbol_field};

/// Real-time latency promise a transport profile makes.
///
/// Ordered loosely from most relaxed to most demanding; the kernel defines the
/// latency contract as [`Symbol`]s and this enum is the concrete set the fabric
/// recognizes. [`LatencyClass::symbol`] and [`LatencyClass::from_symbol`] map to
/// and from the kernel symbol under the `stream/latency` namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LatencyClass {
    /// Offline (faster- or slower-than-real-time) rendering; no timing promise.
    OfflineRender,
    /// Block-local processing latency within a single host.
    BlockLocal,
    /// Interactive latency suitable for responsive control.
    Interactive,
    /// Sample-exact timing (the tightest real-time class).
    SampleExact,
    /// Buffered preview latency, trading immediacy for smoothness.
    BufferedPreview,
    /// Collaboration latency tolerating up to a musical bar of delay.
    CollabBarDelay,
    /// Remote-collaboration latency across a network link.
    RemoteCollaboration,
}

impl LatencyClass {
    /// Returns the stable wire label for this class (for example
    /// `"sample-exact"`).
    pub fn wire_label(self) -> &'static str {
        match self {
            Self::OfflineRender => "offline-render",
            Self::BlockLocal => "block-local",
            Self::Interactive => "interactive",
            Self::SampleExact => "sample-exact",
            Self::BufferedPreview => "buffered-preview",
            Self::CollabBarDelay => "collab-bardelay",
            Self::RemoteCollaboration => "remote-collaboration",
        }
    }

    /// Returns the kernel [`Symbol`] for this class under the `stream/latency`
    /// namespace.
    pub fn symbol(self) -> Symbol {
        Symbol::qualified("stream/latency", self.wire_label())
    }

    /// Parses a [`LatencyClass`] from a kernel [`Symbol`].
    ///
    /// Accepts the bare label and the fully qualified `stream/latency/<label>`
    /// form, erroring on any unrecognized latency class.
    pub fn from_symbol(symbol: &Symbol) -> Result<Self> {
        match symbol.as_qualified_str().as_str() {
            "offline-render" | "stream/latency/offline-render" => Ok(Self::OfflineRender),
            "block-local" | "stream/latency/block-local" => Ok(Self::BlockLocal),
            "interactive" | "stream/latency/interactive" => Ok(Self::Interactive),
            "sample-exact" | "stream/latency/sample-exact" => Ok(Self::SampleExact),
            "buffered-preview" | "stream/latency/buffered-preview" => Ok(Self::BufferedPreview),
            "collab-bardelay" | "stream/latency/collab-bardelay" => Ok(Self::CollabBarDelay),
            "remote-collaboration" | "stream/latency/remote-collaboration" => {
                Ok(Self::RemoteCollaboration)
            }
            other => Err(Error::Eval(format!("unknown stream latency class {other}"))),
        }
    }
}

/// One thing a transport is permitted to do with a stream.
///
/// A [`TransportProfile`] carries a set of these. The kernel defines the
/// capability vocabulary as [`Symbol`]s; this enum is the concrete set the
/// fabric recognizes, mapped under the `stream/capability` namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamCapability {
    /// Sample-exact delivery with no approximation.
    Exact,
    /// Deterministic, reproducible output for identical input.
    Deterministic,
    /// Real-time delivery.
    Realtime,
    /// Bounded buffering and latency.
    Bounded,
    /// Delivery across a remote (networked) link.
    Remote,
    /// Output that can be replayed from a recorded source.
    Replayable,
    /// Lower-fidelity preview output.
    Preview,
    /// Output backed by persistent storage.
    Persistent,
    /// Delivery that can resume after interruption.
    Resumable,
    /// Lossy delivery that may drop or approximate data.
    Lossy,
}

impl StreamCapability {
    /// Returns the stable wire label for this capability (for example
    /// `"realtime"`).
    pub fn wire_label(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Deterministic => "deterministic",
            Self::Realtime => "realtime",
            Self::Bounded => "bounded",
            Self::Remote => "remote",
            Self::Replayable => "replayable",
            Self::Preview => "preview",
            Self::Persistent => "persistent",
            Self::Resumable => "resumable",
            Self::Lossy => "lossy",
        }
    }

    /// Returns the kernel [`Symbol`] for this capability under the
    /// `stream/capability` namespace.
    pub fn symbol(self) -> Symbol {
        Symbol::qualified("stream/capability", self.wire_label())
    }

    /// Parses a [`StreamCapability`] from a kernel [`Symbol`].
    ///
    /// Accepts the bare label and the fully qualified
    /// `stream/capability/<label>` form, erroring on any unrecognized
    /// capability.
    pub fn from_symbol(symbol: &Symbol) -> Result<Self> {
        match symbol.as_qualified_str().as_str() {
            "exact" | "stream/capability/exact" => Ok(Self::Exact),
            "deterministic" | "stream/capability/deterministic" => Ok(Self::Deterministic),
            "realtime" | "stream/capability/realtime" => Ok(Self::Realtime),
            "bounded" | "stream/capability/bounded" => Ok(Self::Bounded),
            "remote" | "stream/capability/remote" => Ok(Self::Remote),
            "replayable" | "stream/capability/replayable" => Ok(Self::Replayable),
            "preview" | "stream/capability/preview" => Ok(Self::Preview),
            "persistent" | "stream/capability/persistent" => Ok(Self::Persistent),
            "resumable" | "stream/capability/resumable" => Ok(Self::Resumable),
            "lossy" | "stream/capability/lossy" => Ok(Self::Lossy),
            other => Err(Error::Eval(format!("unknown stream capability {other}"))),
        }
    }

    /// Returns the latency class this capability implies on its own.
    ///
    /// Used as a default when reasoning about a lone capability; an assembled
    /// [`TransportProfile`] carries its own [`LatencyClass`] independently.
    pub fn latency_class(self) -> LatencyClass {
        match self {
            Self::Exact => LatencyClass::SampleExact,
            Self::Deterministic => LatencyClass::OfflineRender,
            Self::Realtime => LatencyClass::SampleExact,
            Self::Bounded => LatencyClass::BlockLocal,
            Self::Remote => LatencyClass::RemoteCollaboration,
            Self::Replayable => LatencyClass::OfflineRender,
            Self::Preview => LatencyClass::BufferedPreview,
            Self::Persistent => LatencyClass::RemoteCollaboration,
            Self::Resumable => LatencyClass::RemoteCollaboration,
            Self::Lossy => LatencyClass::BufferedPreview,
        }
    }
}

/// The capability and latency contract a transport offers for a stream.
///
/// A profile pairs a name with a [`LatencyClass`] and a set of
/// [`StreamCapability`] values, validated for mutual consistency at
/// construction. The named constructors provide the standard fabric presets;
/// fields are private and read through the accessor methods.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportProfile {
    name: Symbol,
    latency_class: LatencyClass,
    capabilities: Vec<StreamCapability>,
}

impl TransportProfile {
    /// Builds a profile from a name, latency class, and capability set.
    ///
    /// Rejects inconsistent combinations -- for example `Exact` together with
    /// `Lossy`, or `Realtime` under a non-real-time latency class -- returning
    /// an error rather than a profile.
    pub fn new(
        name: Symbol,
        latency_class: LatencyClass,
        capabilities: Vec<StreamCapability>,
    ) -> Result<Self> {
        validate_capabilities(latency_class, &capabilities)?;
        Ok(Self {
            name,
            latency_class,
            capabilities,
        })
    }

    /// Preset profile for in-process, in-memory streaming: block-local latency
    /// with exact, deterministic, bounded, replayable delivery.
    pub fn memory_local() -> Self {
        Self::new(
            Symbol::qualified("stream/profile", "memory-local"),
            LatencyClass::BlockLocal,
            vec![
                StreamCapability::Exact,
                StreamCapability::Deterministic,
                StreamCapability::Bounded,
                StreamCapability::Replayable,
            ],
        )
        .expect("memory-local stream profile is valid")
    }

    /// Preset profile for local real-time audio: sample-exact latency with
    /// exact, real-time, bounded delivery.
    pub fn realtime_local_audio() -> Self {
        Self::new(
            Symbol::qualified("stream/profile", "realtime-local-audio"),
            LatencyClass::SampleExact,
            vec![
                StreamCapability::Exact,
                StreamCapability::Realtime,
                StreamCapability::Bounded,
            ],
        )
        .expect("realtime-local-audio stream profile is valid")
    }

    /// Preset profile for buffered PCM preview: buffered-preview latency with
    /// bounded, preview, lossy delivery.
    pub fn buffered_pcm_preview() -> Self {
        Self::new(
            Symbol::qualified("stream/profile", "buffered-pcm-preview"),
            LatencyClass::BufferedPreview,
            vec![
                StreamCapability::Bounded,
                StreamCapability::Preview,
                StreamCapability::Lossy,
            ],
        )
        .expect("buffered-pcm-preview stream profile is valid")
    }

    /// Preset profile for the remote stream fabric: remote-collaboration
    /// latency with remote, bounded, replayable, resumable delivery.
    pub fn remote_stream_fabric() -> Self {
        Self::new(
            Symbol::qualified("stream/profile", "remote-stream-fabric"),
            LatencyClass::RemoteCollaboration,
            vec![
                StreamCapability::Remote,
                StreamCapability::Bounded,
                StreamCapability::Replayable,
                StreamCapability::Resumable,
            ],
        )
        .expect("remote-stream-fabric stream profile is valid")
    }

    /// Preset profile for LAN MIDI control: interactive latency with remote,
    /// bounded, replayable delivery.
    pub fn lan_midi_control() -> Self {
        Self::new(
            Symbol::qualified("stream/profile", "lan-midi-control"),
            LatencyClass::Interactive,
            vec![
                StreamCapability::Remote,
                StreamCapability::Bounded,
                StreamCapability::Replayable,
            ],
        )
        .expect("lan-midi-control stream profile is valid")
    }

    /// Preset profile for LAN buffered audio preview: buffered-preview latency
    /// with remote, bounded, preview, lossy delivery.
    pub fn lan_buffered_audio_preview() -> Self {
        Self::new(
            Symbol::qualified("stream/profile", "lan-buffered-audio-preview"),
            LatencyClass::BufferedPreview,
            vec![
                StreamCapability::Remote,
                StreamCapability::Bounded,
                StreamCapability::Preview,
                StreamCapability::Lossy,
            ],
        )
        .expect("lan-buffered-audio-preview stream profile is valid")
    }

    /// Preset profile for LAN render return: offline-render latency with remote,
    /// bounded, deterministic, replayable, resumable delivery.
    pub fn lan_render_return() -> Self {
        Self::new(
            Symbol::qualified("stream/profile", "lan-render-return"),
            LatencyClass::OfflineRender,
            vec![
                StreamCapability::Remote,
                StreamCapability::Bounded,
                StreamCapability::Deterministic,
                StreamCapability::Replayable,
                StreamCapability::Resumable,
            ],
        )
        .expect("lan-render-return stream profile is valid")
    }

    /// Returns the profile's name symbol.
    pub fn name(&self) -> &Symbol {
        &self.name
    }

    /// Returns the latency class this profile promises.
    pub fn latency_class(&self) -> LatencyClass {
        self.latency_class
    }

    /// Returns the capabilities this profile grants.
    pub fn capabilities(&self) -> &[StreamCapability] {
        &self.capabilities
    }

    /// Returns whether this profile grants `capability`.
    pub fn has_capability(&self, capability: StreamCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Encodes this profile into its [`Expr`] map wire form.
    ///
    /// Round-trips back through [`TransportProfile::from_expr`].
    pub fn to_expr(&self) -> Expr {
        Expr::Map(vec![
            (
                Expr::Symbol(Symbol::new("name")),
                Expr::Symbol(self.name.clone()),
            ),
            (
                Expr::Symbol(Symbol::new("latency-class")),
                Expr::Symbol(self.latency_class.symbol()),
            ),
            (
                Expr::Symbol(Symbol::new("capabilities")),
                Expr::List(
                    self.capabilities
                        .iter()
                        .map(|capability| Expr::Symbol(capability.symbol()))
                        .collect(),
                ),
            ),
        ])
    }

    /// Decodes a profile from its [`Expr`] map wire form.
    ///
    /// Requires exactly the `name`, `latency-class`, and `capabilities` fields,
    /// and re-applies the capability/latency consistency checks via
    /// [`TransportProfile::new`].
    pub fn from_expr(expr: &Expr) -> Result<Self> {
        let Expr::Map(entries) = expr else {
            return Err(Error::TypeMismatch {
                expected: "stream transport profile map",
                found: expr_kind(expr),
            });
        };
        ensure_fields(entries, &["name", "latency-class", "capabilities"])?;
        Self::new(
            symbol_field(entries, "name")?.clone(),
            LatencyClass::from_symbol(symbol_field(entries, "latency-class")?)?,
            symbol_list(entries, "capabilities")?
                .iter()
                .map(StreamCapability::from_symbol)
                .collect::<Result<Vec<_>>>()?,
        )
    }
}

fn validate_capabilities(
    latency_class: LatencyClass,
    capabilities: &[StreamCapability],
) -> Result<()> {
    let has = |needle| capabilities.contains(&needle);
    if has(StreamCapability::Exact) && has(StreamCapability::Lossy) {
        return Err(Error::Eval(
            "stream capabilities exact and lossy cannot be combined".to_owned(),
        ));
    }
    if latency_class == LatencyClass::SampleExact && has(StreamCapability::Remote) {
        return Err(Error::Eval(
            "remote streams cannot claim sample-exact latency".to_owned(),
        ));
    }
    if latency_class == LatencyClass::RemoteCollaboration && has(StreamCapability::Realtime) {
        return Err(Error::Eval(
            "remote-collaboration streams cannot claim realtime capability".to_owned(),
        ));
    }
    if latency_class == LatencyClass::OfflineRender && has(StreamCapability::Realtime) {
        return Err(Error::Eval(
            "offline-render streams cannot claim realtime capability".to_owned(),
        ));
    }
    Ok(())
}

fn symbol_list(entries: &[(Expr, Expr)], name: &str) -> Result<Vec<Symbol>> {
    list_field(entries, name)?
        .iter()
        .map(|expr| match expr {
            Expr::Symbol(symbol) => Ok(symbol.clone()),
            other => Err(Error::TypeMismatch {
                expected: "symbol list item",
                found: expr_kind(other),
            }),
        })
        .collect()
}

fn list_field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> Result<&'a [Expr]> {
    match field(entries, name)? {
        Expr::List(items) => Ok(items),
        other => Err(Error::TypeMismatch {
            expected: "list field",
            found: expr_kind(other),
        }),
    }
}

fn ensure_fields(entries: &[(Expr, Expr)], allowed: &[&str]) -> Result<()> {
    for (key, _) in entries {
        let Expr::Symbol(symbol) = key else {
            return Err(Error::TypeMismatch {
                expected: "symbol stream profile field",
                found: expr_kind(key),
            });
        };
        if symbol.namespace.is_none() && allowed.contains(&symbol.name.as_ref()) {
            continue;
        }
        return Err(Error::Eval(format!(
            "unknown stream profile field {}",
            symbol.as_qualified_str()
        )));
    }
    Ok(())
}
