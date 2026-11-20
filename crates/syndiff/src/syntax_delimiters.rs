use smallvec::SmallVec;

/// Shared opener state of the two cursors.
///
/// The walk enters matching delimiters together and unmatched ones one side at
/// a time, so it must remember how deep it is and how many one-sided openers
/// are pending at each level. This used to be packed into two `u128`s (eight
/// bits per nesting level), which panicked or silently corrupted the state as
/// soon as code nested more than sixteen levels deep — generated JSON/JS and
/// C++ templates reach that easily.
///
/// Frames are stored on the stack up to a generous depth and spill to the heap
/// beyond it, so nesting is no longer bounded.
#[derive(Debug, Default, Clone, Hash, PartialEq, Eq)]
pub struct SyntaxDelimiters {
    /// One-sided openers at the outermost level.
    base_lhs: u32,
    base_rhs: u32,
    /// One frame per shared level entered with [`push_both`](Self::push_both);
    /// the last frame is the current level.
    levels: SmallVec<[(u32, u32); 8]>,
}

impl SyntaxDelimiters {
    pub fn is_empty(&self) -> bool {
        self.levels.is_empty() && self.base_lhs == 0 && self.base_rhs == 0
    }

    /// Number of shared delimiters entered together.
    pub fn depth(&self) -> usize {
        self.levels.len()
    }

    pub fn push_both(&self) -> Self {
        let mut next = self.clone();
        next.levels.push((0, 0));
        next
    }

    pub fn push_lhs(&self) -> Self {
        let mut next = self.clone();
        match next.levels.last_mut() {
            Some((lhs, _)) => *lhs = lhs.saturating_add(1),
            None => next.base_lhs = next.base_lhs.saturating_add(1),
        }
        next
    }

    pub fn push_rhs(&self) -> Self {
        let mut next = self.clone();
        match next.levels.last_mut() {
            Some((_, rhs)) => *rhs = rhs.saturating_add(1),
            None => next.base_rhs = next.base_rhs.saturating_add(1),
        }
        next
    }

    pub fn pop_lhs(&self) -> Option<Self> {
        if !self.can_pop_lhs() {
            return None;
        }
        let mut next = self.clone();
        match next.levels.last_mut() {
            Some((lhs, _)) => *lhs -= 1,
            None => next.base_lhs -= 1,
        }
        Some(next)
    }

    pub fn pop_rhs(&self) -> Option<Self> {
        if !self.can_pop_rhs() {
            return None;
        }
        let mut next = self.clone();
        match next.levels.last_mut() {
            Some((_, rhs)) => *rhs -= 1,
            None => next.base_rhs -= 1,
        }
        Some(next)
    }

    pub fn pop_both(&self) -> Option<Self> {
        if self.levels.last() != Some(&(0, 0)) {
            return None;
        }
        let mut next = self.clone();
        next.levels.pop();
        Some(next)
    }

    pub fn can_pop_lhs(&self) -> bool {
        match self.levels.last() {
            Some((lhs, _)) => *lhs > 0,
            None => self.base_lhs > 0,
        }
    }

    pub fn can_pop_rhs(&self) -> bool {
        match self.levels.last() {
            Some((_, rhs)) => *rhs > 0,
            None => self.base_rhs > 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SyntaxDelimiters;

    #[test]
    fn tracks_one_sided_delimiters_per_level() {
        let d = SyntaxDelimiters::default();
        assert!(d.is_empty());

        let d = d.push_lhs();
        assert!(!d.is_empty());
        assert!(d.can_pop_lhs());
        assert!(!d.can_pop_rhs());

        let d = d.push_rhs();
        assert!(!d.pop_lhs().unwrap().can_pop_lhs());
        assert!(d.pop_rhs().is_some());

        let d = d.pop_lhs().unwrap().pop_rhs().unwrap();
        assert!(d.is_empty());
        assert!(d.pop_both().is_none());
    }

    #[test]
    fn shared_nesting_is_unbounded() {
        let mut d = SyntaxDelimiters::default();
        for _ in 0..1024 {
            d = d.push_both();
        }
        assert_eq!(d.depth(), 1024);
        assert!(!d.is_empty());
        assert!(!d.can_pop_lhs());

        for _ in 0..1024 {
            d = d.pop_both().expect("balanced nesting pops");
        }
        assert!(d.is_empty());
    }

    #[test]
    fn pop_both_requires_current_level_to_be_clear() {
        let d = SyntaxDelimiters::default().push_both().push_lhs();
        assert!(d.pop_both().is_none());
        assert!(d.pop_lhs().unwrap().pop_both().is_some());
    }

    #[test]
    fn levels_are_independent() {
        let d = SyntaxDelimiters::default()
            .push_lhs()
            .push_both()
            .push_rhs();
        assert!(d.can_pop_rhs() && !d.can_pop_lhs());
        let d = d.pop_rhs().unwrap().pop_both().unwrap();
        assert!(d.can_pop_lhs() && !d.can_pop_rhs());
        assert!(d.pop_lhs().unwrap().is_empty());
    }
}
