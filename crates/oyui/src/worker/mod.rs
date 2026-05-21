use oyui_tasker::register_tasker;

pub mod context;

pub mod tasks {
    pub mod full_diff;
    pub mod stats;
    pub mod syntax;
}

register_tasker! {
    tasks = [
        Stats    => tasks::stats::Stats,
        FullDiff => tasks::full_diff::FullDiff,
        Syntax   => tasks::syntax::Syntax,
    ]
}
