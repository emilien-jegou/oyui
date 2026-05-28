pub mod context;

pub mod tasks {
    pub mod calculate_file_tree;
    pub mod full_diff;
    pub mod stats;
    pub mod syntax;
    pub mod watch_config;
}

use oyui_tasker::tasker_registry;

tasker_registry! {
    events = [
        CalculateFileTree    => tasks::calculate_file_tree::CalculateFileTreeReq,
        CalculateFileTreeRes => tasks::calculate_file_tree::CalculateFileTreeRes,
        Stats                => tasks::stats::StatsReq,
        StatsRes             => tasks::stats::StatsRes,
        FullDiff             => tasks::full_diff::FullDiffReq,
        FullDiffRes          => tasks::full_diff::FullDiffRes,
        Syntax               => tasks::syntax::SyntaxReq,
        SyntaxRes            => tasks::syntax::SyntaxRes,
        WatchConfig          => tasks::watch_config::WatchConfigReq,
        WatchConfigRes       => tasks::watch_config::WatchConfigRes,
    ],
    listeners = [
        CalculateFileTree    => [tasks::calculate_file_tree::CalculateFileTree],
        CalculateFileTreeRes => [tasks::calculate_file_tree::CalculateFileTreeResListener],
        Stats                => [tasks::stats::Stats],
        StatsRes             => [tasks::stats::StatsResListener],
        FullDiff             => [tasks::full_diff::FullDiff],
        FullDiffRes          => [tasks::full_diff::FullDiffResListener],
        Syntax               => [tasks::syntax::Syntax],
        SyntaxRes            => [tasks::syntax::SyntaxResListener],
        WatchConfig          => [tasks::watch_config::WatchConfig],
    ],
}
