#[derive(Debug, Clone, Default)]
pub struct DevIconProvider;

impl super::FileIconProvider for DevIconProvider {
    fn get_file_icon(&self, name: &str) -> char {
        let lowercase_name = name.to_ascii_lowercase();

        if let Some(icon) = Self::get_exact_match(&lowercase_name) {
            return icon;
        }

        if let Some(icon) = Self::get_special_suffix_match(&lowercase_name) {
            return icon;
        }

        let ext = name.split('.').next_back().unwrap_or("");
        Self::get_extension_match(&ext.to_ascii_lowercase())
    }
}

impl DevIconProvider {
    fn get_exact_match(name: &str) -> Option<char> {
        match name {
            ".babelrc" => Some(''),
            ".bash_profile" | ".bashrc" | ".zprofile" | ".zshenv" | ".zshrc" => Some(''),
            ".dockerignore"
            | "containerfile"
            | "dockerfile"
            | "docker-compose.yaml"
            | "docker-compose.yml"
            | "compose.yaml"
            | "compose.yml" => Some('󰡨'),
            ".ds_store" | ".gitconfig" | ".luaurc" => Some(''),
            ".editorconfig" => Some(''),
            ".env" | "env" => Some(''),
            ".eslintrc" | ".eslintignore" | "eslint.config.cjs" | "eslint.config.js"
            | "eslint.config.mjs" | "eslint.config.ts" => Some(''),
            ".git-blame-ignore-revs"
            | ".gitattributes"
            | ".gitignore"
            | ".gitmodules"
            | "commit_editmsg" => Some(''),
            ".gitlab-ci.yml" => Some(''),
            ".gtkrc-2.0" | "gtkrc" => Some(''),
            ".gvimrc" | ".vimrc" | "_gvimrc" | "_vimrc" => Some(''),
            ".justfile" | "justfile" => Some(''),
            ".mailmap" => Some('󰊢'),
            ".npmignore" | ".npmrc" => Some(''),
            ".nuxtrc" => Some('󱄆'),
            ".nvmrc" => Some(''),
            ".prettierrc"
            | ".prettierrc.json"
            | ".prettierrc.json5"
            | ".prettierrc.toml"
            | ".prettierrc.yaml"
            | ".prettierrc.yml"
            | ".prettierignore"
            | "prettier.config.js"
            | "prettier.config.cjs"
            | "prettier.config.mjs"
            | "prettier.config.ts" => Some(''),
            ".settings.json" => Some(''),
            ".srcinfo" | ".SRCINFO" => Some('󰣇'),
            ".xauthority" | ".xinitrc" | ".xresources" | ".xsession" | "xorg.conf"
            | "xsettingsd.conf" => Some(''),
            "brewfile" | "gemfile$" => Some(''),
            "bspwmrc" | "sxhkdrc" => Some(''),
            "build" | "bazel" | "bzl" | "workspace" => Some(''),
            "build.gradle" | "gradlew" | "gradle.properties" | "gradle-wrapper.properties" => {
                Some('')
            }
            "build.zig.zon" => Some(''),
            "checkhealth" => Some('󰓙'),
            "cmakelists.txt" | "config" | "cmake" => Some(''),
            "code_of_conduct" | "code_of_conduct.md" => Some(''),
            "commitlint.config.js" | "commitlint.config.ts" => Some('󰜘'),
            "copying" | "copying.lesser" => Some(''),
            "ext_typoscript_setup.txt" => Some(''),
            "favicon.ico" => Some(''),
            "fp-info-cache" | "fp-lib-table" | "sym-lib-table" => Some(''),
            "freecad.conf" | "FreeCAD.conf" => Some(''),
            "gnumakefile" | "makefile" => Some(''),
            "go.mod" | "go.sum" | "go.work" => Some(''),
            "gruntfile.babel.js" | "gruntfile.coffee" | "gruntfile.js" | "gruntfile.ts" => {
                Some('')
            }
            "gulpfile.babel.js" | "gulpfile.coffee" | "gulpfile.js" | "gulpfile.ts" => Some(''),
            "hypridle.conf" | "hyprland.conf" | "hyprlock.conf" => Some(''),
            "i18n.config.js" | "i18n.config.ts" => Some('󰗊'),
            "i3blocks.conf" | "i3status.conf" => Some(''),
            "ionic.config.json" => Some(''),
            "cantorrc" | "kalgebrarc" | "kdeglobals" => Some(''),
            "kdenlive-layoutsrc" | "kdenliverc" => Some(''),
            "kritadisplayrc" | "kritarc" => Some(''),
            "license" | "license.md" => Some(''),
            "lxde-rc.xml" => Some(''),
            "lxqt.conf" => Some(''),
            "mix.lock" => Some(''),
            "mpv.conf" => Some(''),
            "node_modules" => Some(''),
            "nuxt.config.cjs" | "nuxt.config.js" | "nuxt.config.mjs" | "nuxt.config.ts" => {
                Some('󱄆')
            }
            "package.json" | "package-lock.json" => Some(''),
            "pkgbuild" | "PKGBUILD" => Some(''),
            "platformio.ini" => Some(''),
            "pom.xml" => Some(''),
            "procfile" => Some(''),
            "prusaslicer.ini"
            | "PrusaSlicer.ini"
            | "prusaslicergcodeviewer.ini"
            | "PrusaSlicerGcodeViewer.ini" => Some(''),
            "py.typed" => Some(''),
            "qtproject.conf" | "QtProject.conf" => Some(''),
            "rakefile" => Some(''),
            "robots.txt" => Some('󰚩'),
            "security" | "security.md" => Some('󰒃'),
            "settings.gradle" => Some(''),
            "svelte.config.js" => Some(''),
            "unlicense" => Some(''),
            "vagrantfile$" => Some(''),
            "vlcrc" => Some('󰕼'),
            "vercel.json" => Some('▲'),
            "webpack" => Some('󰜫'),
            "weston.ini" => Some(''),
            _ => None,
        }
    }

    fn get_special_suffix_match(name: &str) -> Option<char> {
        if name.ends_with(".spec.js")
            || name.ends_with(".spec.jsx")
            || name.ends_with(".spec.ts")
            || name.ends_with(".spec.tsx")
            || name.ends_with(".test.js")
            || name.ends_with(".test.jsx")
            || name.ends_with(".test.ts")
            || name.ends_with(".test.tsx")
        {
            return Some('');
        }
        None
    }

    fn get_extension_match(ext: &str) -> char {
        match ext {
            // Source/Programming Languages
            "rs" => '',
            "rlib" => '',
            "c" | "m" => '',
            "c++" | "cc" | "ccm" | "cp" | "cpp" | "cppm" | "cxx" | "cxxm" | "mm" | "mpp"
            | "ixx" => '',
            "cs" => '󰌛',
            "java" => '',
            "kt" | "kts" => '',
            "swift" | "xcplayground" => '',
            "go" => '',
            "zig" => '',
            "nim" => '',
            "nix" => '',
            "scala" | "sbt" | "sc" => '',
            "lua" | "luac" | "luau" => '',
            "php" => '',
            "pl" | "pm" | "t" => '',
            "rb" | "rake" => '',
            "ex" | "exs" | "eex" | "heex" | "leex" => '',
            "erl" | "hrl" => '',
            "clj" | "cljc" => '',
            "cljs" | "cljd" | "edn" => '',
            "fs" | "fsi" | "fsscript" | "fsx" | "f#" => '',
            "hs" | "lhs" => '',
            "ml" | "mli" => '',
            "sml" | "sig" | "signature" => 'λ',
            "el" | "elc" | "eln" => '',
            "apl" => '⍝',
            "bqn" => '⎉',
            "elm" => '',
            "gleam" => '',
            "vala" => '',
            "sol" => '',
            "hx" => '',
            "mojo" | "🔥" => '',
            "fnl" => '',
            "nu" => '>',
            "org" => '',
            "scm" => '󰘧',
            "f90" => '󱈚',
            "r" => '󰟔',
            "groovy" => '',

            // Python Family
            "py" | "pyc" | "pyd" | "pyi" | "pyo" | "pyw" | "pyx" | "ipynb" | "pxd" | "pxi" => '',

            // Scripting & Terminal
            "sh" | "bash" | "zsh" | "fish" | "csh" | "ksh" | "awk" => '',
            "bat" => '',
            "ps1" | "psd1" | "psm1" => '󰨊',
            "tcl" | "tbc" => '󰛓',
            "azcli" => '',
            "x" | "xm" => '',

            // Web Development
            "js" | "cjs" | "mjs" => '',
            "ts" | "cts" | "mts" | "d.ts" => '',
            "jsx" => '',
            "tsx" => '',
            "vue" => '',
            "svelte" => '',
            "astro" => '',
            "html" => '',
            "htm" => '',
            "css" => '',
            "sass" | "scss" => '',
            "less" => '',
            "styl" => '',
            "liquid" => '',
            "templ" => '',

            // Data / Config / Markup
            "json" | "json5" | "jsonc" | "cson" | "webmanifest" | "nswag" => '',
            "toml" => '',
            "yaml" | "yml" | "cfg" | "conf" | "ini" => '',
            "xml" | "xaml" => '󰗀',
            "csv" => '',
            "tf" => '',
            "tfvars" => '',
            "bib" => '󱉟',
            "tex" => '',
            "tsconfig" => '',

            // Compiled Binaries & Libraries
            "o" | "out" | "bin" | "elf" | "exe" | "app" => '',
            "so" | "a" | "ko" | "dll" | "lib" => '',
            "wasm" => '',

            // Document formats
            "txt" => '󰈙',
            "pdf" => '',
            "doc" | "docx" => '󰈬',
            "xls" | "xlsx" => '󰈛',
            "ppt" => '󰈧',
            "epub" | "mobi" | "ebook" => '',
            "markdown" | "md" | "mdx" | "rmd" => '',

            // Images & Graphics
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "jxl" | "ico" => '',
            "svg" => '󰜡',
            "ai" => '',
            "psd" | "psb" => '',
            "xcf" => '',
            "image" | "img" | "iso" => '',

            // Audio & Playlists
            "mp3" | "m4a" | "wav" | "flac" | "ogg" | "opus" | "aac" | "aif" | "aiff" | "ape"
            | "pcm" | "wma" | "wv" | "wvc" => '',
            "cue" | "m3u" | "m3u8" | "pls" => '󰲹',

            // Video
            "mp4" | "mkv" | "mov" | "webm" | "3gp" | "m4v" | "cast" => '',

            // Archives / Compressed
            "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "zst" | "tgz" | "txz" | "bz"
            | "bz3" => '',

            // CAD, 3D Models & Makers
            "dwg" | "dxf" | "ifc" | "ige" | "iges" | "igs" | "skp" | "sldasm" | "sldprt"
            | "slvs" | "ste" | "step" | "stp" | "brep" | "f3d" => '󰻫',
            "3mf" | "fbx" | "obj" | "ply" | "stl" | "wrl" | "wrz" => '󰆧',
            "gcode" => '󰐫',
            "fcbak" | "fcmacro" | "fcmat" | "fcparam" | "fcscript" | "fcstd" | "fcstd1"
            | "fctb" | "fctl" => '',
            "scad" => '',

            // Localization & Translation
            "po" | "pot" | "qm" | "strings" | "xcstrings" => '',

            // Configuration & Package Suffixes
            "lock" | "lck" => '',
            "log" => '󰌱',
            "bak" => '󰁯',
            "cache" => '',
            "config.ru" => '',
            "gnumakefile" | "makefile" | "mk" => '',
            "webpack" => '󰜫',

            // Fallback default file icon
            _ => '󰈚',
        }
    }
}
