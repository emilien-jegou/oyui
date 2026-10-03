use std::sync::Arc;
use syntect::parsing::SyntaxSet;

#[derive(Clone)]
pub struct SyntaxEngine {
    pub syntax_set: Arc<SyntaxSet>,
}

impl Default for SyntaxEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxEngine {
    pub fn new() -> Self {
        let syntax_set = Arc::new(two_face::syntax::extra_newlines());
        Self { syntax_set }
    }
}
