{
  languages = {
    typescript.enable = true;
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
    echo "testing for typescript"
  '';
}
