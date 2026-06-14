pub mod context;

pub mod events {
    pub mod diff_update;
    pub mod file_opened;
    pub mod file_syntax_update;
    pub mod theme_update;
}

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
        Syntax               => tasks::syntax::SyntaxReq,
        WatchConfig          => tasks::watch_config::WatchConfigReq,
        WatchConfigRes       => tasks::watch_config::WatchConfigRes,
        DiffUpdate           => events::diff_update::DiffUpdate,
        FileSyntaxUpdate     => events::file_syntax_update::FileSyntaxUpdate,
        FileOpened           => events::file_opened::FileOpened,
        ThemeUpdate          => events::theme_update::ThemeUpdate,
    ],
    listeners = [
        CalculateFileTree    => [tasks::calculate_file_tree::CalculateFileTree],
        CalculateFileTreeRes => [tasks::calculate_file_tree::CalculateFileTreeResListener],
        Stats                => [tasks::stats::Stats],
        StatsRes             => [tasks::stats::StatsResListener],
        FullDiff             => [tasks::full_diff::FullDiff],
        Syntax               => [tasks::syntax::Syntax],
        SyntaxRes            => [tasks::syntax::SyntaxResListener],
        WatchConfig          => [tasks::watch_config::WatchConfig],
        FileOpened           => [tasks::syntax::Syntax],
        DiffUpdate           => [tasks::syntax::Syntax],
        ThemeUpdate          => [tasks::syntax::Syntax],
    ],
}
