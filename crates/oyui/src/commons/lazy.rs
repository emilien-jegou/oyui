#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub enum Lazy<T> {
    #[default]
    Uninitialized,
    Started,
    Ready(T),
    Stale(T),
    StaleRestarted(T),
}

impl<T> Lazy<T> {
    /// Helper to transition a ready/stale state into `Stale` (invalidated),
    /// or keep it as `Unstarted` if it never had a value.
    pub fn invalidate(&mut self) {
        let prev = std::mem::replace(self, Lazy::Uninitialized);
        match prev {
            Lazy::Ready(v) | Lazy::Stale(v) | Lazy::StaleRestarted(v) => {
                *self = Lazy::Stale(v);
            }
            _ => *self = Lazy::Uninitialized,
        }
    }

    /// Access the underlying value if it exists, even if stale.
    pub fn value(&self) -> Option<&T> {
        match self {
            Lazy::Ready(v) | Lazy::Stale(v) | Lazy::StaleRestarted(v) => Some(v),
            _ => None,
        }
    }
}
