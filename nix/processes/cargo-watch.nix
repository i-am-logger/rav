{ ... }:
{
  processes = {
    cargo-watch = {
      exec = "cargo watch -x run";
    };
  };
}
