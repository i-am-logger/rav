{
  languages = {
    go = {
      enable = true;

      # workaround required for Delve debugger (https://github.com/go-delve/delve/issues/3085
      # enableHardeningWorkaround = true;

    };
  };

  git-hooks.hooks = {
    # gofmt.enable = true;
    # golines.enable = true;
    # gptcommit.enable = true;
  };

  enterTest = ''
    echo "testing for go"
    # govet
  '';
}
