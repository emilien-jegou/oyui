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
      cmd = "release";
      desc = "Run release script";
      exec = "bun ./scripts/release";
    }
    {
      cmd = "theme-screen";
      desc = "Generate the theme.md file";
      exec = "./scripts/theme-screen/script.ts";
    }
  ];
}
