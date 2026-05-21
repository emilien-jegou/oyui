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
  ];
}
