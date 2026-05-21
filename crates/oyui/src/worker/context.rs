use crate::{cli::Opts, syntax::SyntaxEngine};
use oyui_tasker::TaskerProvide;
use typed_builder::TypedBuilder;

#[derive(TypedBuilder, TaskerProvide, Clone)]
pub struct AppWorkerContext {
    pub syntax_engine: SyntaxEngine,
    pub config: Opts,
}
