{
  languages = {
    javascript = {
      enable = true;
      npm = {
        enable = true;
        #   install.enable = true;
      };
      pnpm = {
        enable = true;
        install.enable = true;
      };
    };
  };

  git-hooks.hooks = {
    # prettier = {
    #   enable = true;
    #   settings.configPath = "./ prettier.config.js";
    #   files = [
    #     "**/*.ts"
    #   ];
    # };
  };

  enterTest = ''
    echo "testing for javascript"
  '';
}
