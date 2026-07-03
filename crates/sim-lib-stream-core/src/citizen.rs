//! Object/class citizenship for stream packets in the runtime.
//!
//! [`StreamPacketDescriptor`] is the runtime citizen wrapping a
//! [`StreamPacket`]: it derives `Citizen` under the `stream/Packet` class so a
//! packet can live as a first-class object, and stores the packet in its
//! encoded [`Expr`] form while validating that the expr decodes back to a
//! packet. [`stream_packet_class_symbol`] returns the class symbol that
//! identifies these citizens.

use sim_citizen_derive::Citizen;
use sim_kernel::{Expr, Result, Symbol};

use crate::{DataPacket, StreamPacket};

/// Runtime citizen wrapping a [`StreamPacket`] under the `stream/Packet` class.
///
/// The packet is held in its encoded [`Expr`] form; construction validates that
/// the expr round-trips to a [`StreamPacket`].
#[derive(Clone, Debug, PartialEq, Citizen)]
#[citizen(symbol = "stream/Packet", version = 1)]
pub struct StreamPacketDescriptor {
    #[citizen(with = "packet_expr")]
    packet: Expr,
}

impl StreamPacketDescriptor {
    /// Wraps a [`StreamPacket`], storing its encoded [`Expr`] form.
    pub fn new(packet: StreamPacket) -> Self {
        Self {
            packet: packet.to_expr(),
        }
    }

    /// Wraps an already-encoded packet [`Expr`], validating that it decodes to a
    /// [`StreamPacket`].
    ///
    /// Returns an error if the expression is not a valid encoded stream packet.
    pub fn from_expr(expr: Expr) -> Result<Self> {
        packet_expr::decode(&expr)?;
        Ok(Self { packet: expr })
    }

    /// Decodes and returns the wrapped [`StreamPacket`].
    ///
    /// Returns an error if the stored expression no longer decodes to a packet.
    pub fn packet(&self) -> Result<StreamPacket> {
        StreamPacket::try_from(self.packet.clone())
    }

    /// Returns the wrapped packet in its encoded [`Expr`] form.
    pub fn as_expr(&self) -> &Expr {
        &self.packet
    }
}

impl Default for StreamPacketDescriptor {
    fn default() -> Self {
        Self::new(StreamPacket::Data(DataPacket::new(
            Symbol::qualified("stream/data", "citizen-fixture"),
            Expr::String("packet".to_owned()),
        )))
    }
}

/// Returns the `stream/Packet` class symbol that identifies stream-packet
/// citizens.
pub fn stream_packet_class_symbol() -> Symbol {
    Symbol::qualified("stream", "Packet")
}

pub(crate) mod packet_expr {
    use sim_kernel::{Expr, Result};

    use crate::StreamPacket;

    pub fn encode(expr: &Expr) -> Expr {
        expr.clone()
    }

    pub fn decode(expr: &Expr) -> Result<Expr> {
        StreamPacket::try_from(expr.clone())?;
        Ok(expr.clone())
    }
}
