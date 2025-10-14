//! Event types and signal definitions for the QP framework

use core::fmt;

/// Type-safe event signal identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct QSignal(pub u16);

impl QSignal {
    /// Reserved signal for framework initialization
    pub const INIT: QSignal = QSignal(0);
    /// Reserved signal for state entry actions  
    pub const ENTRY: QSignal = QSignal(1);
    /// Reserved signal for state exit actions
    pub const EXIT: QSignal = QSignal(2);
    /// Reserved signal for empty/null events
    pub const EMPTY: QSignal = QSignal(3);
    
    /// First user-defined signal
    pub const USER: QSignal = QSignal(4);
    
    /// Create a new signal from a raw value
    pub const fn new(signal: u16) -> Self {
        QSignal(signal)
    }
    
    /// Get the raw signal value
    pub const fn raw(self) -> u16 {
        self.0
    }
}

impl fmt::Display for QSignal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "QSignal({})", self.0)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for QSignal {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "QSignal({})", self.0);
    }
}

/// Base trait for all events in the QP framework
pub trait QEvent: Send + Sync + 'static {
    /// Get the signal identifier for this event
    fn signal(&self) -> QSignal;
    
    /// Check if this is a reserved framework event
    fn is_reserved(&self) -> bool {
        self.signal().0 < QSignal::USER.0
    }
}

/// Static event that carries no data
#[derive(Debug, Clone, Copy)]
pub struct QStaticEvent {
    pub signal: QSignal,
}

impl QStaticEvent {
    /// Create a new static event
    pub const fn new(signal: QSignal) -> Self {
        Self { signal }
    }
}

impl QEvent for QStaticEvent {
    fn signal(&self) -> QSignal {
        self.signal
    }
}

/// Dynamic event that can carry arbitrary data
pub struct QDynamicEvent<T> {
    pub signal: QSignal,
    pub data: T,
}

impl<T> QDynamicEvent<T> {
    /// Create a new dynamic event with data
    pub const fn new(signal: QSignal, data: T) -> Self {
        Self { signal, data }
    }
}

impl<T: Send + Sync + 'static> QEvent for QDynamicEvent<T> {
    fn signal(&self) -> QSignal {
        self.signal
    }
}

impl<T: fmt::Debug> fmt::Debug for QDynamicEvent<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QDynamicEvent")
            .field("signal", &self.signal)
            .field("data", &self.data)
            .finish()
    }
}

/// Event reference for passing events between active objects
pub enum QEventRef<'a> {
    Static(&'a QStaticEvent),
    Dynamic(&'a dyn QEvent),
}

impl<'a> QEventRef<'a> {
    /// Get the signal from the event reference
    pub fn signal(&self) -> QSignal {
        match self {
            QEventRef::Static(evt) => evt.signal(),
            QEventRef::Dynamic(evt) => evt.signal(),
        }
    }
}

/// Macro to define custom event enums that implement QEvent
#[macro_export]
macro_rules! define_events {
    (
        $vis:vis enum $name:ident {
            $(
                $variant:ident $(($data:ty))? = $signal:expr
            ),* $(,)?
        }
    ) => {
        #[derive(Debug)]
        $vis enum $name {
            $(
                $variant $(($data))?,
            )*
        }
        
        impl $crate::QEvent for $name {
            fn signal(&self) -> $crate::QSignal {
                match self {
                    $(
                        $name::$variant $(_(_))? => $crate::QSignal($signal),
                    )*
                }
            }
        }
    };
}