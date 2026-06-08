{
  name = "dev";
  desc = "Oyui Development Helper";
  scripts = [
    {
      cmd = "build";
      desc = "Build the project";
      exec = "cargo build";
    }
    {
      cmd = "test";
      desc = "Run cargo tests";
      exec = "cargo test";
    }
    {
      cmd = "watch";
      desc = "Run bacon";
      exec = "bacon";
    }
    {
      cmd = "release:prepare";
      desc = "Install dependencies in release folder";
      visible = false;
      dir = "./scripts/release";
      exec = "bun install";
    }
    {
      cmd = "release";
      desc = "Generate release commit and git tags";
      deps = ["release:prepare"];
      exec = "bun ./scripts/release/release.ts";
    }
    {
      cmd = "release-github";
      desc = "Generate release patch for github and deploy changes";
      deps = ["release:prepare"];
      exec = "bun ./scripts/release/release-gh.ts";
    }
    {
      cmd = "theme-screen";
      desc = "Generate the theme.md file";
      exec = ["./scripts/theme-screen/script.ts"];
    }
  ];
}
